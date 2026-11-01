//! The gates of the input queue (#1353 design, section 2.2).
//!
//! A gate closes the queue from one position on: the entry it was opened at, and everything
//! behind it, wait. Every gate has a named EVENT that opens it again; the optional cap is only a
//! give-up safety net for an event that may never arrive (it is requested through
//! `Host::arm_give_up`, fires at most once and is guarded by the gate's token), never the thing
//! correctness depends on.

use std::time::Duration;

use super::EntryId;

/// What a gate waits for. Each variant names the legacy buffer it will replace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GateKind {
    /// A pty- or editor-bound paste is being read off-thread (Q4, Q4b).
    PasteReading,
    /// An Enter waits for the autodetect verdict or an in-flight Tab completion (Q1).
    EnterVerdict,
    /// The hand-off asked the shell for its line (`ESC i`) and waits for the report (Q3a).
    HandoffReport,
    /// An Enter just ran as a shell command and the hand-off route is still settling (Q2).
    PtyRouteSettling,
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
    /// The editor took focus from the pty and has not been focused yet.
    EditorFocusPending,
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
    pub const ALL: [GateKind; 11] = [
        GateKind::PasteReading,
        GateKind::EnterVerdict,
        GateKind::HandoffReport,
        GateKind::PtyRouteSettling,
        GateKind::PromptSeen,
        GateKind::LineEditorActive,
        GateKind::SubshellSetup,
        GateKind::PowershellSetup,
        GateKind::SubshellBootstrapWrite,
        GateKind::SshConnect,
        GateKind::EditorFocusPending,
    ];

    pub fn name(self) -> &'static str {
        match self {
            GateKind::PasteReading => "paste_reading",
            GateKind::EnterVerdict => "enter_verdict",
            GateKind::HandoffReport => "handoff_report",
            GateKind::PtyRouteSettling => "pty_route_settling",
            GateKind::PromptSeen => "prompt_seen",
            GateKind::LineEditorActive => "line_editor_active",
            GateKind::SubshellSetup => "subshell_setup",
            GateKind::PowershellSetup => "powershell_setup",
            GateKind::SubshellBootstrapWrite => "subshell_bootstrap_write",
            GateKind::SshConnect => "ssh_connect",
            GateKind::EditorFocusPending => "editor_focus_pending",
        }
    }

    /// The named event that closes the queue on this gate (for the log and the doc table).
    pub fn opened_by(self) -> &'static str {
        match self {
            GateKind::PasteReading => "begin_paste",
            GateKind::EnterVerdict => "enter_without_settled_verdict",
            GateKind::HandoffReport => "handoff_report_requested",
            GateKind::PtyRouteSettling => "enter_ran_as_shell_command",
            GateKind::PromptSeen => "young_enter",
            GateKind::LineEditorActive => "precmd_on_conpty",
            GateKind::SubshellSetup => "subshell_setup_announced",
            GateKind::PowershellSetup => "powershell_setup_announced",
            GateKind::SubshellBootstrapWrite => "bootstrap_write_started",
            GateKind::SshConnect => "ssh_connect_started",
            GateKind::EditorFocusPending => "editor_took_over",
        }
    }

    /// The named event that opens the gate again. These are what the owner rule wants correctness
    /// to hang on.
    pub fn closing_event(self) -> &'static str {
        match self {
            GateKind::PasteReading => "paste_read_result",
            GateKind::EnterVerdict => "verdict_ready_or_tab_completion_done",
            GateKind::HandoffReport => "report_received",
            GateKind::PtyRouteSettling => "handoff_engaged_or_pty_owned",
            GateKind::PromptSeen => "precmd_parsed_or_command_running",
            GateKind::LineEditorActive => "line_editor_active",
            GateKind::SubshellSetup => "bootstrapped_then_precmd_or_abandoned",
            GateKind::PowershellSetup => "powershell_prompt_ready",
            GateKind::SubshellBootstrapWrite => "last_bootstrap_line_written",
            GateKind::SshConnect => "gate_released_and_first_remote_precmd",
            GateKind::EditorFocusPending => "editor_focused_or_pointer_down",
        }
    }

    /// The signal rule per gate kind: the table of design 2.5 as code, so a new gate cannot
    /// forget to decide.
    pub fn signal_plan(self) -> SignalPlan {
        match self {
            GateKind::PasteReading => SignalPlan::CancelPasteThenFlush,
            GateKind::EnterVerdict | GateKind::SshConnect => SignalPlan::DiscardThenSignal,
            GateKind::HandoffReport
            | GateKind::PtyRouteSettling
            | GateKind::PromptSeen
            | GateKind::LineEditorActive
            | GateKind::EditorFocusPending => SignalPlan::FlushThenSignal,
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
            GateKind::EnterVerdict => Some(Duration::from_millis(250)),
            GateKind::HandoffReport => Some(Duration::from_secs(3)),
            GateKind::PtyRouteSettling => Some(Duration::from_millis(300)),
            GateKind::PromptSeen => Some(Duration::from_millis(600)),
            // The hand-off's give-up cap since #1349 (the line editor's own signal opens it).
            GateKind::LineEditorActive => Some(Duration::from_millis(1500)),
            GateKind::SubshellSetup | GateKind::PowershellSetup => Some(Duration::from_secs(90)),
            GateKind::SubshellBootstrapWrite => None,
            GateKind::SshConnect => Some(Duration::from_secs(5)),
            GateKind::EditorFocusPending => Some(Duration::from_secs(1)),
        }
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
