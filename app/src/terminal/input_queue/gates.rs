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
    pub const ALL: [GateKind; 12] = [
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
            GateKind::PromptSeen => "precmd_parsed_or_command_running",
            GateKind::LineEditorActive => "line_editor_active",
            GateKind::SubshellSetup => "bootstrapped_then_precmd_or_abandoned",
            GateKind::PowershellSetup => "powershell_prompt_ready",
            GateKind::SubshellBootstrapWrite => "last_bootstrap_line_written",
            GateKind::SshConnect => "gate_released_and_first_remote_precmd",
            GateKind::EditorDisplayMap => "replay_step_dispatched",
        }
    }

    /// The signal rule per gate kind: the table of design 2.5 as code, so a new gate cannot
    /// forget to decide.
    pub fn signal_plan(self) -> SignalPlan {
        match self {
            // A submit holds no paste entry, so the plan only flushes what was typed before the
            // signal, as it did when the submit's gate was a `PasteReading` one.
            GateKind::PasteReading | GateKind::CliSubmit => SignalPlan::CancelPasteThenFlush,
            GateKind::EnterVerdict | GateKind::TabCompletion | GateKind::SshConnect => {
                SignalPlan::DiscardThenSignal
            }
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
    /// the opener must supply one (the bootstrap gate's cap is N lines x 2 s).
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
            GateKind::SubshellSetup | GateKind::PowershellSetup => Some(Duration::from_secs(90)),
            GateKind::SubshellBootstrapWrite => None,
            GateKind::SshConnect => Some(Duration::from_secs(5)),
            // The deferred replay step runs as soon as this update's effects have flushed; this
            // only guards a step that never runs (logged when it fires).
            GateKind::EditorDisplayMap => Some(Duration::from_secs(1)),
        }
    }
}

/// What a signal does at a gate, per signal (#1424, OD-1): the plan depends on the signal as well
/// as on the gate, because the tty flushes pending input on INTR, QUIT and SUSP but not on EOF.
/// The legacy [`SignalPlan`] is per gate only and stays what `InputQueue::apply_signal` runs; this
/// is the table's form, with the cases the legacy code handles outside the queue (the hand-off's
/// deferred Enter, the subshell bootstrap stash, the held keys' void) named.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    /// The signal is stashed (`pending_aborts`) and sent after the last bootstrap line: it must
    /// not cut a half-run script.
    StashUntilBootstrapDone,
}

/// The plan for `signal` meeting `gate` (the table's [`GateKind::signal_plan`], per signal).
/// Interrupt and Eof are what the legacy code does; the Suspend and Quit columns are the deliberate
/// OD-1 change (^Z and ^\ jump the queue like ^C). The ssh setup window is NOT a case of this
/// function: inside it the table's P1a row holds the signal instead (OD-5).
pub fn signal_plan(gate: GateKind, signal: SignalKind) -> GateSignalPlan {
    let eof = signal == SignalKind::Eof;
    match gate {
        GateKind::PasteReading | GateKind::CliSubmit => GateSignalPlan::CancelPasteThenFlush,
        GateKind::EnterVerdict | GateKind::TabCompletion | GateKind::SshConnect => {
            GateSignalPlan::DiscardThenSignal
        }
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
