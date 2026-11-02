//! The destination table (#1424, design revision 2 + 3): where one input goes, as ONE pure function.
//!
//! `decide(intent, &RouteFacts, pin) -> Plan { outcome, effects, on_refused, row }` replaces, in
//! later steps, the chain of predicates that is spread over `route_typed`, `keydown_on_terminal`,
//! `control_sequence_on_terminal`, `hold_input_during_ssh_connect`, `write_user_bytes_to_pty_inner`,
//! `after_paste_stage`, `deliver_replayed_entry`, `deliver_ssh_held` and the editor's hold paths.
//! **Nothing calls it yet** (steps 1 and 2: facts plumbing and the pure table); legacy stays in
//! charge and `PHOSPHOR_QUEUE_DESTINATION` defaults to `legacy`.
//!
//! Pure means: no clock, no lock, no I/O, no allocation, no view state. Everything it needs is in
//! the [`Intent`] (what the input is, where it came from, the [`Stamp`] it got at the sink) and the
//! [`RouteFacts`] (one `Copy` value). The one impure thing in the design, the typeahead hand-off's
//! oracle, is not here: a `Pty` outcome carries [`Effects::CONSULT_ORACLE`] and the host consults
//! it (once, under one guard) when it executes the plan. The hand-off's pure `peek` lives in
//! `TypeaheadHandoff::peek_user_bytes_route`.
//!
//! # Rows, in the order they are asked
//!
//! For content (text, Enter, control keys, escape sequences, pastes, raw bytes, editor keys):
//!
//! | row | when | outcome |
//! |---|---|---|
//! | `OSynced` | origin Synced, not (bootstrapped and long running) | `Drop(NotLongRunning)` |
//! | `D13` | agent write, setup gate or ssh remote setup | `Drop(AgentRefusedSetup)`, else `Pty(Agent)` |
//! | `OCliSubmit` | origin CliSubmit: exempt from every hold | `Pty(CliSubmit)` or `Drop(AgentControl)` |
//! | `H1` | Typed/Paste, stamped focus is the ssh choice card | `Hint` |
//! | `H4` | ssh prompt up, the stamp is older than `prompt_gen` (OD-7) | `Drop(StaleBeforePrompt)` |
//! | `H2` | Typed/Enter/Control/Paste, the ssh setup dialog is up | `Swallow(SshDialogOpen)` |
//! | `H3b` | an SshConnect gate is open but the phase is not Holding | effect only: `SSH_HOLD_DETACHED` |
//! | `H3` | the ssh phase is Holding (editor keys too) | `Hold(SshConnect)` |
//! | `H5` | Typed/Enter/editor key, an EnterVerdict or TabCompletion gate is open | `Hold(that gate)` |
//! | `H6` | PowerShell setup holds keys | `Hold(PowershellSetup)` |
//! | `H7` / `H7b` | subshell setup gate / bootstrap write gate; or the shell asks for an answer (bypass) | `Hold(..)` / skip H7, H8 |
//! | `H8` | any other gate, queued entries or the mailbox | `Hold(gate)` / `Queue`; see [`gate_holds`] |
//! | `H10` | editor key needing a fresh display map | `Hold(EditorDisplayMap)` |
//! | `D9` | origin Editor | `EditorInsert` / `EditorAction` / `EditorSubmit` |
//! | `D1` / `D2` | approval card focus / the agent owns the pty | `AgentInput` |
//! | `D1'` / `D11` | replayed Enter: not pty / pty | `EditorSubmit` / `Pty(EnterReplay)` |
//! | `D7` | Typed, `!pty_typed` | `EditorInsert` (always: OD-4, B12) |
//! | `D4` / `D5` | alt screen take-over; the agent still in control | effect / `Drop(AgentControl)` or editor insert |
//! | `D6` | the class's pty predicate holds | `Pty(..)` / `PasteToPty` + `CONSULT_ORACLE` |
//! | `D8` | Typed, `pty_typed`, the pty refuses, the box is hidden | `on_refused = Drop(RefusedHiddenInput)` |
//! | `D10` | Control/Seq/live Enter, the class's pty predicate is false | `EditorAction`/`EditorSubmit` or `Drop(NotLongRunning)` |
//! | `D3` / `D12` | raw bytes: the agent in control / else | `Drop(AgentControl)` / `Pty(Passive)` |
//! | `Pin` | a line pinned to the editor | `EditorInsert` instead of `Pty` |
//!
//! Signals (^C ^D ^Z ^\, OD-1) never wait behind a gate; their rows are `P1f` (a held signal
//! delivered to the remote), `P1a` (OD-5: held in the ssh setup window and delivered REMOTELY),
//! `P1b` (effect: the ssh hold is reset), `P1c` (PowerShell setup), the pty predicate / `D10`, `P1e`
//! (^C while ssh connects: `~.`, SIGINT to the foreground group, or the byte), `P1d` (stashed during
//! the subshell bootstrap lines), `P2` (the queue applies [`signal_plan`] per gate), `D6`.
//!
//! # The four pty predicates (B11)
//!
//! `pty_typed`, `pty_control`, `pty_seq` and `pty_enter_replay` are exactly what legacy computes per
//! class. Unifying them is a deliberate change (B11) that is NOT made here; the differences are
//! tested one by one (a block started but not yet long running, pre-bootstrap with a pending
//! session, a shared-session executor, a stalled subshell bootstrap, the agent owning the pty while
//! the hand-off routes to the pty).

use super::facts::{FocusSnap, RouteFacts, SshPhase, Stamp, SubshellPhase};
use super::{DestFacts, Destination, DropReason, EntryKind, GateKind, Origin, SignalKind};

pub use super::gates::{GateSignalPlan, signal_plan};

/// What kind of input is being decided. The class is the input's shape; where it came from is
/// [`Intent::origin`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// Printable text.
    Typed,
    Enter,
    /// A control key other than Enter and the signals (Backspace, Ctrl-U, Tab ...).
    Control,
    /// A control sequence (arrows, Home/End ...).
    Seq,
    /// ^C, ^D, ^Z or ^\ (OD-1: all four jump the queue).
    Signal(SignalKind),
    Paste,
    /// Passive pty bytes (mouse reports, wheel to arrows).
    Raw,
    /// Bytes the agent writes to its command.
    AgentWrite,
    /// A key for the input editor (text or action).
    EditKey,
}

impl Class {
    fn is_signal(self) -> bool {
        matches!(self, Class::Signal(_))
    }

    /// The classes the ssh rows (H1-H4) judge.
    fn is_ssh_judged(self) -> bool {
        matches!(
            self,
            Class::Typed | Class::Enter | Class::Control | Class::Seq | Class::Paste
        )
    }
}

/// One input to decide. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Intent {
    pub class: Class,
    pub origin: Origin,
    /// What was true when the key reached the sink (H4, D1, H1).
    pub stamp: Stamp,
    /// Keyboard-driven bytes take agent control of an alt-screen program (D4); pointer-driven
    /// bytes (mouse, wheel) do not.
    pub takes_over_agent: bool,
    /// The entry is being delivered from the queue (it already has its place): the order holds
    /// (`Queue`) do not apply, only the gates that still cover it.
    pub delivering: bool,
    /// An editor key that reads the display map (cursor motion, deletion by display position).
    pub needs_display_map: bool,
    /// The editor's display map has caught up with its edits.
    pub display_map_fresh: bool,
    /// A signal is flushing the queue: the editor's keys are applied at once (H10).
    pub flushing: bool,
}

impl Intent {
    pub fn new(class: Class, origin: Origin) -> Self {
        Self {
            class,
            origin,
            stamp: Stamp::UNSTAMPED,
            takes_over_agent: true,
            delivering: false,
            needs_display_map: false,
            display_map_fresh: false,
            flushing: false,
        }
    }

    pub fn stamped(mut self, stamp: Stamp) -> Self {
        self.stamp = stamp;
        self
    }

    pub fn delivering(mut self) -> Self {
        self.delivering = true;
        self
    }

    pub fn passive(mut self) -> Self {
        self.takes_over_agent = false;
        self
    }

    fn is_replay_enter_origin(&self) -> bool {
        matches!(
            self.origin,
            Origin::Replay(_) | Origin::Held | Origin::ReleasedFrom(_)
        )
    }
}

/// How a `Pty` outcome writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PtyWrite {
    Typed,
    Control,
    Seq,
    EnterReplay,
    /// Passive bytes (mouse reports, synced relays): no `pty_takes_input` test, no hidden-box drop.
    Passive,
    Agent,
    CliSubmit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwallowReason {
    /// The ssh setup dialog owns the keyboard.
    SshDialogOpen,
}

/// Why an input was dropped. Every `Drop` outcome carries one, and the executor logs the row and
/// the reason (counts only, never the content).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropWhy {
    /// The agent is in control of the command (D3, D5).
    AgentControl,
    /// Not a running command and no editor takeover to take it (D10, OSynced).
    NotLongRunning,
    /// The pty refuses the bytes and the input box is hidden (D5/D8).
    RefusedHiddenInput,
    /// The agent's write met a setup gate (D13).
    AgentRefusedSetup,
    /// The key was stamped before the ssh prompt was published (H4, OD-7): it may be a password
    /// typed blind.
    StaleBeforePrompt,
}

/// What the plan does with the input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Write to the pty through the hand-off oracle.
    Pty(PtyWrite),
    /// A paste to the pty.
    PasteToPty {
        bracketed: bool,
    },
    /// A signal to the pty through the leaf (never held again).
    SignalToPty(SignalKind),
    /// ^C while ssh connects: SIGINT to the foreground group.
    SigintForeground,
    /// ^C between the ssh hook and the remote session: the `~.` abort bytes.
    AbortBytes,
    EditorInsert,
    EditorAction,
    EditorSubmit,
    /// The agent's input (steer text, an approval card follow-up).
    AgentInput,
    /// Held behind a gate of this kind.
    Hold(GateKind),
    /// A signal held in the ssh segment (`HELD_SIGNAL`, OD-5), delivered to the remote at the
    /// hand-over.
    HoldSignal,
    /// Behind queued entries or the mailbox, with no gate of its own.
    Queue,
    /// A signal meets open gates: the queue applies [`signal_plan`] for each.
    SignalAtGates,
    /// Stashed until the last subshell bootstrap line is written (`pending_aborts`).
    Stash,
    /// ^C during a PowerShell setup (`ctrl_c_during_powershell_setup`).
    PowershellAbort,
    /// The ssh choice card shows its "choose an option first" hint.
    Hint,
    Swallow(SwallowReason),
    Drop(DropWhy),
}

impl Outcome {
    /// The outcome writes to the pty.
    pub fn is_pty(&self) -> bool {
        matches!(
            self,
            Outcome::Pty(_)
                | Outcome::PasteToPty { .. }
                | Outcome::SignalToPty(_)
                | Outcome::SigintForeground
                | Outcome::AbortBytes
        )
    }

    pub fn is_drop(&self) -> bool {
        matches!(self, Outcome::Drop(_) | Outcome::Swallow(_))
    }

    pub fn is_hold(&self) -> bool {
        matches!(
            self,
            Outcome::Hold(_) | Outcome::HoldSignal | Outcome::Queue | Outcome::Stash
        )
    }
}

/// What the pty refusing the write means, for `Pty(Typed)` and `PasteToPty` (D8): the text goes
/// to the editor, or is dropped when the input box is hidden.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnRefused {
    NotApplicable,
    EditorInsert,
    Drop(DropWhy),
}

/// Side effects the executor performs, as a bit set (plain data, `Copy`).
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Effects(u32);

impl Effects {
    pub const NONE: Effects = Effects(0);
    /// The ssh hold is noted (a toast once, the hold flag).
    pub const NOTE_SSH_HOLDING: Effects = Effects(1 << 0);
    /// Arm the ssh give-up timer (a logged net, never what correctness depends on).
    pub const ARM_SSH_GIVE_UP: Effects = Effects(1 << 1);
    /// A passing interrupt resets the ssh hold (the tty's own input flush on ^C).
    pub const RESET_SSH_HOLD: Effects = Effects(1 << 2);
    /// The queue holds an SshConnect gate the view no longer has: drop that gate segment, then go on.
    pub const SSH_HOLD_DETACHED: Effects = Effects(1 << 3);
    pub const REFOCUS_DIALOG: Effects = Effects(1 << 4);
    /// Show the "typing is ignored" toast once.
    pub const TOAST_ONCE: Effects = Effects(1 << 5);
    /// Show "Interrupt held until the SSH session finishes starting" (now true: OD-5).
    pub const NOTE_INTERRUPT_HELD: Effects = Effects(1 << 6);
    pub const FOCUS_INPUT: Effects = Effects(1 << 7);
    /// After a successful pty write, focus the terminal (a settled AI target had focus).
    pub const FOCUS_TERMINAL: Effects = Effects(1 << 8);
    pub const REFOCUS_GLOBAL: Effects = Effects(1 << 9);
    pub const CLEAR_SELECTION: Effects = Effects(1 << 10);
    /// Hand an alt-screen program the agent controls to the user, then re-read the facts (D4).
    pub const SWITCH_CONTROL_TO_USER: Effects = Effects(1 << 11);
    /// Consult the typeahead hand-off (once, under one guard). Absent for released entries.
    pub const CONSULT_ORACLE: Effects = Effects(1 << 12);
    /// The SIGINT path marks the ssh command interrupted (`leave_user_wrote(ctrl_c)`).
    pub const LEAVE_USER_WROTE_CTRL_C: Effects = Effects(1 << 13);
    /// The write result feeds `ObserveCtrlC` (shared-session viewer bytes).
    pub const OBSERVE_CTRL_C: Effects = Effects(1 << 14);
    /// Drop the rest of a chunked line-clear write (`CancelPacedSetupWrites`).
    pub const CANCEL_PACED_SETUP_WRITES: Effects = Effects(1 << 15);
    /// The drain yields after this entry: a command may have been submitted, and the next entry
    /// must be judged with its effects applied (#1421, every `EditorSubmit` and Enter to the pty).
    pub const YIELD_AFTER: Effects = Effects(1 << 16);
    /// The choice card is told a key was typed before the choice.
    pub const NOTE_CHOICE_CARD_HINT: Effects = Effects(1 << 17);

    pub const fn union(self, other: Effects) -> Effects {
        Effects(self.0 | other.0)
    }

    pub const fn contains(self, other: Effects) -> bool {
        self.0 & other.0 == other.0 && other.0 != 0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl std::ops::BitOr for Effects {
    type Output = Effects;

    fn bitor(self, rhs: Effects) -> Effects {
        self.union(rhs)
    }
}

impl std::fmt::Debug for Effects {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Effects({:#x})", self.0)
    }
}

/// The row that decided, for the log and the shadow comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    OSynced,
    OCliSubmit,
    H1,
    H2,
    H3,
    H3b,
    H4,
    H5,
    H6,
    H7,
    H7b,
    H8,
    H8Queue,
    H10,
    P1a,
    P1b,
    P1c,
    P1d,
    P1e,
    P1f,
    P2,
    D1,
    D1Replay,
    D2,
    D2b,
    D3,
    D4,
    D5,
    D6,
    D7,
    D9,
    D10,
    D11,
    D12,
    D13,
    Pin,
}

impl Row {
    pub fn name(self) -> &'static str {
        match self {
            Row::OSynced => "O-synced",
            Row::OCliSubmit => "O-cli-submit",
            Row::H1 => "H1",
            Row::H2 => "H2",
            Row::H3 => "H3",
            Row::H3b => "H3b",
            Row::H4 => "H4",
            Row::H5 => "H5",
            Row::H6 => "H6",
            Row::H7 => "H7",
            Row::H7b => "H7b",
            Row::H8 => "H8",
            Row::H8Queue => "H8-queue",
            Row::H10 => "H10",
            Row::P1a => "P1a",
            Row::P1b => "P1b",
            Row::P1c => "P1c",
            Row::P1d => "P1d",
            Row::P1e => "P1e",
            Row::P1f => "P1f",
            Row::P2 => "P2",
            Row::D1 => "D1",
            Row::D1Replay => "D1'",
            Row::D2 => "D2",
            Row::D2b => "D2b",
            Row::D3 => "D3",
            Row::D4 => "D4",
            Row::D5 => "D5",
            Row::D6 => "D6",
            Row::D7 => "D7",
            Row::D9 => "D9",
            Row::D10 => "D10",
            Row::D11 => "D11",
            Row::D12 => "D12",
            Row::D13 => "D13",
            Row::Pin => "pin",
        }
    }
}

/// The table's answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    pub outcome: Outcome,
    pub effects: Effects,
    /// What a refused pty write turns into (D8).
    pub on_refused: OnRefused,
    pub row: Row,
}

impl Plan {
    fn new(outcome: Outcome, row: Row, effects: Effects) -> Self {
        Self {
            outcome,
            effects,
            on_refused: OnRefused::NotApplicable,
            row,
        }
    }

    fn refused(mut self, on_refused: OnRefused) -> Self {
        self.on_refused = on_refused;
        self
    }

    fn with(mut self, effects: Effects) -> Self {
        self.effects = self.effects | effects;
        self
    }
}

// ---- the four pty predicates (B11) ----------------------------------------------------------

/// `should_write_typed_chars_to_pty`: the block started (not necessarily long running: the span
/// between Enter and preexec counts) or the hand-off routes keys to the pty, AND the bootstrap
/// script was echoed (a session is pending or bootstrapped), or a shared-session executor, or the
/// subshell bootstrap stalled.
pub fn pty_typed(f: &RouteFacts) -> bool {
    if !f.active_block_started && !f.handoff_routes {
        return false;
    }
    f.session_pending_or_bootstrapped || f.shared_session_executor || f.subshell_bootstrap_stalled
}

/// `keydown_on_terminal`'s pty test for a control key. `abort` is ^C / ^D (the legacy
/// `is_abort_key`): those are not blocked by the agent owning the pty, are forwarded during a
/// subshell bootstrap at once, and reach the pty when the ssh bootstrap route is Forward.
pub fn pty_control(f: &RouteFacts, abort: bool) -> bool {
    let agent_owns_key = !abort && f.agent_owns_pty;
    let forwards_subshell = if abort {
        f.forwards_subshell_abort
    } else {
        f.forwards_subshell_key
    };
    (f.long_running && !agent_owns_key)
        || f.handoff_routes
        || forwards_subshell
        || (abort && f.ssh_route_forward)
}

/// `control_sequence_on_terminal`'s pty test (the agent-owned case is row D2b, before this).
pub fn pty_seq(f: &RouteFacts) -> bool {
    f.long_running || f.handoff_routes || f.forwards_subshell_key
}

/// `replayed_enter_goes_to_pty`: the text before the Enter went to the pty.
pub fn pty_enter_replay(f: &RouteFacts) -> bool {
    !(f.approval_card || f.ai_awaiting_confirmation || f.agent_owns_pty)
        && (pty_typed(f) || f.long_running || f.handoff_routes)
}

// ---- the gate x class matrix (H8) -----------------------------------------------------------

/// Whether the open gates hold an input of `class` and `origin` (the one matrix, replacing the two
/// places that used to state it). `gates` is `f.gates`; the answer is false when none is open.
///
/// Every gate holds every key (strict FIFO), with the two exemptions the code had in two places:
///
/// * **only the ssh gate is open** (`only_ssh_gates_open`): the ssh hold holds its own keys (row H3
///   decided before this one), so what reaches the matrix is a deliberate pass-through, or a key
///   of a gate the view has already detached (H3b).
/// * **editor keys behind only a subshell / PowerShell setup gate** (`editor_keys_pass_gates`,
///   #1423): the setup holds PTY bytes, not editor keys, so they apply at once. Only while
///   everything queued is pty-bound (`queued_only_pty`).
///
/// Signals never reach this function: they apply [`signal_plan`] per gate instead.
pub fn gate_holds(f: &RouteFacts, class: Class, origin: Origin) -> bool {
    debug_assert!(
        !class.is_signal(),
        "signals apply signal_plan, not the matrix"
    );
    if f.gates.is_empty() {
        return false;
    }
    if f.gates.all_within(&[GateKind::SshConnect]) {
        return false;
    }
    let editor_exempt = origin == Origin::Editor
        && f.gates
            .all_within(&[GateKind::SubshellSetup, GateKind::PowershellSetup])
        && f.queued_only_pty;
    !editor_exempt
}

/// The gate an input waits behind: the first open one in [`GateKind::ALL`] order (a stable
/// choice; the queue itself holds it behind all of them).
fn first_gate(f: &RouteFacts) -> GateKind {
    f.gates.kinds().next().unwrap_or(GateKind::SshConnect)
}

// ---- decide ---------------------------------------------------------------------------------

/// Decides where `intent` goes. `pin` is the destination of the line being typed (line-run
/// pinning, design 2.3 rule 6): a line is not split across destinations by a fact that flipped
/// mid-line.
pub fn decide(intent: &Intent, f: &RouteFacts, pin: Option<Destination>) -> Plan {
    let plan = match intent.class {
        Class::Signal(signal) => decide_signal(intent, f, signal),
        _ => decide_content(intent, f),
    };
    apply_pin(plan, intent, f, pin)
}

fn drop_plan(why: DropWhy, row: Row, effects: Effects) -> Plan {
    Plan::new(Outcome::Drop(why), row, effects)
}

/// D4 and D5 for a pty-bound input: an alt-screen program the agent controls is taken over by
/// keyboard-driven bytes (effect, and the facts are re-read: modelled here as the take-over
/// succeeding), and while the agent is still in control the pty refuses the write.
/// Returns `(agent_still_in_control, effects)`.
fn take_over(intent: &Intent, f: &RouteFacts) -> (bool, Effects) {
    if f.alt_screen && f.agent_in_control && intent.takes_over_agent {
        (false, Effects::SWITCH_CONTROL_TO_USER)
    } else {
        (f.agent_in_control, Effects::NONE)
    }
}

/// A pty plan with the oracle effect (not for entries released from a hand-off gate: they have
/// been through it once and must not re-enter it) and the origin effects.
fn pty_plan(intent: &Intent, outcome: Outcome, row: Row, effects: Effects) -> Plan {
    let mut fx = effects;
    if !matches!(intent.origin, Origin::ReleasedFrom(_)) {
        fx = fx | Effects::CONSULT_ORACLE;
    }
    if intent.origin == Origin::Viewer {
        fx = fx | Effects::OBSERVE_CTRL_C;
    }
    Plan::new(outcome, row, fx)
}

fn decide_content(i: &Intent, f: &RouteFacts) -> Plan {
    let class = i.class;
    // ---- origin rows that come before anything ----
    if i.origin == Origin::Synced && !(f.is_bootstrapped && f.long_running) {
        return drop_plan(DropWhy::NotLongRunning, Row::OSynced, Effects::NONE);
    }
    if class == Class::AgentWrite {
        return if f.agent_write_gate_closed {
            drop_plan(DropWhy::AgentRefusedSetup, Row::D13, Effects::NONE)
        } else {
            pty_plan(i, Outcome::Pty(PtyWrite::Agent), Row::D13, Effects::NONE)
        };
    }
    if i.origin == Origin::CliSubmit {
        // Exempt from every hold; still through D4/D5 and the oracle, as `after_paste_stage`.
        let (agent, fx) = take_over(i, f);
        return if agent {
            drop_plan(DropWhy::AgentControl, Row::D5, fx)
        } else {
            pty_plan(i, Outcome::Pty(PtyWrite::CliSubmit), Row::OCliSubmit, fx)
        };
    }

    // ---- hold rows ----
    let mut fx = Effects::NONE;
    let ssh_judged = class.is_ssh_judged();
    if matches!(class, Class::Typed | Class::Paste) && i.stamp.focus == FocusSnap::SshChoiceCard {
        return Plan::new(Outcome::Hint, Row::H1, Effects::NOTE_CHOICE_CARD_HINT);
    }
    if ssh_judged
        && matches!(i.origin, Origin::Keyboard | Origin::Editor | Origin::Held)
        && let SshPhase::PromptUp { prompt_gen } = f.ssh
        && i.stamp.is_before(prompt_gen)
    {
        return drop_plan(DropWhy::StaleBeforePrompt, Row::H4, Effects::NONE);
    }
    if matches!(
        class,
        Class::Typed | Class::Enter | Class::Paste | Class::Control
    ) && f.ssh_dialog_up
    {
        return Plan::new(
            Outcome::Swallow(SwallowReason::SshDialogOpen),
            Row::H2,
            Effects::REFOCUS_DIALOG | Effects::TOAST_ONCE,
        );
    }
    // The editor's keys join the ssh hold too (`offer_ssh_held_key`), so H3 and H3b judge them.
    let ssh_holds = ssh_judged || class == Class::EditKey;
    if ssh_holds && f.queue_has_ssh_gate && f.ssh != SshPhase::Holding {
        // The view detached its state before the deferred drop ran: do not hold behind the stale
        // gate, drop that segment and go on to the next rows (legacy view.rs:11067-11071).
        fx = fx | Effects::SSH_HOLD_DETACHED;
    }
    if ssh_holds && f.ssh == SshPhase::Holding {
        return Plan::new(
            Outcome::Hold(GateKind::SshConnect),
            Row::H3,
            fx | Effects::NOTE_SSH_HOLDING | Effects::ARM_SSH_GIVE_UP,
        );
    }
    if matches!(class, Class::Typed | Class::Enter | Class::EditKey) {
        for gate in [GateKind::EnterVerdict, GateKind::TabCompletion] {
            if f.gates.has(gate) {
                return Plan::new(Outcome::Hold(gate), Row::H5, fx);
            }
        }
    }
    let editor_origin = i.origin == Origin::Editor;
    if !editor_origin && f.ps.holds_keys() {
        return Plan::new(Outcome::Hold(GateKind::PowershellSetup), Row::H6, fx);
    }
    // H7b: the shell asks for something and only subshell/PowerShell gates are open: typing is
    // the answer, so the subshell holds (and the matrix) let it through.
    let only_setup_gates = f.gates.is_empty()
        || f.gates.all_within(&[
            GateKind::SubshellSetup,
            GateKind::SubshellBootstrapWrite,
            GateKind::PowershellSetup,
        ]);
    let bypass_holds = f.subshell_awaiting_answer
        && only_setup_gates
        && f.subshell != SubshellPhase::BootstrapWriting
        && !f.ps.is_active();
    if !bypass_holds {
        if !editor_origin {
            match f.subshell {
                SubshellPhase::SetupHeld => {
                    return Plan::new(Outcome::Hold(GateKind::SubshellSetup), Row::H7, fx);
                }
                SubshellPhase::BootstrapWriting => {
                    return Plan::new(Outcome::Hold(GateKind::SubshellBootstrapWrite), Row::H7, fx);
                }
                SubshellPhase::Idle | SubshellPhase::Announced => {}
            }
        }
        if gate_holds(f, class, i.origin) {
            return Plan::new(Outcome::Hold(first_gate(f)), Row::H8, fx);
        }
        if !i.delivering && (f.mailbox_non_empty || (f.queued && f.gates.is_empty())) {
            return Plan::new(Outcome::Queue, Row::H8Queue, fx);
        }
    } else if !i.delivering && f.mailbox_non_empty {
        // The bypass lets a key past the subshell gates, never past earlier operations.
        return Plan::new(Outcome::Queue, Row::H8Queue, fx);
    }
    if class == Class::EditKey && i.needs_display_map && !i.flushing && !i.display_map_fresh {
        return Plan::new(Outcome::Hold(GateKind::EditorDisplayMap), Row::H10, fx);
    }

    // ---- destination rows ----
    destination(i, f, fx)
}

/// Rows D1-D12, after the holds. `fx` carries the effects the hold rows left.
fn destination(i: &Intent, f: &RouteFacts, fx: Effects) -> Plan {
    let class = i.class;
    // D9: what the input editor produced is the editor's.
    if i.origin == Origin::Editor || class == Class::EditKey {
        let outcome = match class {
            Class::Typed | Class::Paste => Outcome::EditorInsert,
            Class::Enter => Outcome::EditorSubmit,
            _ => Outcome::EditorAction,
        };
        let yield_after = if outcome == Outcome::EditorSubmit {
            Effects::YIELD_AFTER
        } else {
            Effects::NONE
        };
        return Plan::new(outcome, Row::D9, fx | yield_after);
    }
    match class {
        Class::Typed => destination_typed(i, f, fx),
        Class::Paste => destination_paste(i, f, fx),
        Class::Enter => destination_enter(i, f, fx),
        Class::Control => destination_control(i, f, fx),
        Class::Seq => destination_seq(i, f, fx),
        Class::Raw => {
            // D3 / D12. Passive bytes drop only when the agent is IN CONTROL (OD-3); the old
            // `destination_for` also dropped on `agent_owns_pty`, which legacy does not.
            if f.agent_in_control {
                drop_plan(DropWhy::AgentControl, Row::D3, fx)
            } else {
                pty_plan(i, Outcome::Pty(PtyWrite::Passive), Row::D12, fx)
            }
        }
        Class::Signal(_) | Class::AgentWrite | Class::EditKey => {
            unreachable!("decided before the destination rows")
        }
    }
}

fn hidden_or_insert(f: &RouteFacts) -> OnRefused {
    if f.input_box_visible {
        OnRefused::EditorInsert
    } else {
        OnRefused::Drop(DropWhy::RefusedHiddenInput)
    }
}

fn destination_typed(i: &Intent, f: &RouteFacts, fx: Effects) -> Plan {
    // D1: an approval card or an AI block awaiting confirmation had focus at the sink.
    if matches!(
        i.stamp.focus,
        FocusSnap::ApprovalCard | FocusSnap::AiBlockAwaitingConfirm
    ) {
        return Plan::new(Outcome::AgentInput, Row::D1, fx | Effects::FOCUS_INPUT);
    }
    // D2: the agent drives the running command: typing is its steer text.
    if f.agent_owns_pty {
        return Plan::new(Outcome::AgentInput, Row::D2, fx);
    }
    let settled = if i.stamp.focus == FocusSnap::SettledAiTarget {
        Effects::FOCUS_INPUT
    } else {
        Effects::NONE
    };
    // D7: the pty does not take typed text yet (not started, not bootstrapped): the editor, whether
    // or not the box is visible. NEVER a drop (B12; OD-4).
    if !pty_typed(f) {
        return Plan::new(Outcome::EditorInsert, Row::D7, fx | settled);
    }
    // D4 / D5: the agent still owns the command: the text goes to the box if it is shown.
    let (agent, take_over_fx) = take_over(i, f);
    if agent {
        let fx = fx | take_over_fx;
        return if f.input_box_visible {
            Plan::new(Outcome::EditorInsert, Row::D5, fx | settled)
        } else {
            drop_plan(DropWhy::RefusedHiddenInput, Row::D5, fx)
        };
    }
    // D6 (+ D8 on refusal).
    let focus = if i.stamp.focus == FocusSnap::SettledAiTarget {
        Effects::FOCUS_TERMINAL
    } else {
        Effects::NONE
    };
    pty_plan(
        i,
        Outcome::Pty(PtyWrite::Typed),
        Row::D6,
        fx | take_over_fx | focus,
    )
    .refused(hidden_or_insert(f))
}

fn destination_paste(i: &Intent, f: &RouteFacts, fx: Effects) -> Plan {
    if matches!(
        i.stamp.focus,
        FocusSnap::ApprovalCard | FocusSnap::AiBlockAwaitingConfirm
    ) {
        return Plan::new(Outcome::AgentInput, Row::D1, fx | Effects::FOCUS_INPUT);
    }
    if f.agent_owns_pty {
        return Plan::new(Outcome::AgentInput, Row::D2, fx);
    }
    // `paste_routing`: the input box when it is shown and either focused or the shell is
    // bootstrapped, unless the hand-off routes to the pty.
    if f.input_box_visible && (f.editor_focused || f.is_bootstrapped) && !f.handoff_routes {
        return Plan::new(Outcome::EditorInsert, Row::D7, fx);
    }
    let (agent, take_over_fx) = take_over(i, f);
    if agent {
        // The result of a paste to a refused pty is ignored by legacy (the text is lost
        // silently); the drop carries a reason so it is logged.
        return if f.input_box_visible {
            Plan::new(Outcome::EditorInsert, Row::D5, fx | take_over_fx)
        } else {
            drop_plan(DropWhy::RefusedHiddenInput, Row::D5, fx | take_over_fx)
        };
    }
    pty_plan(
        i,
        Outcome::PasteToPty {
            bracketed: f.needs_bracketed_paste,
        },
        Row::D6,
        fx | take_over_fx,
    )
}

fn destination_enter(i: &Intent, f: &RouteFacts, fx: Effects) -> Plan {
    let yield_after = Effects::YIELD_AFTER;
    if i.is_replay_enter_origin() {
        // D1' / D11: a replayed or held Enter goes where the text before it went.
        if !pty_enter_replay(f) {
            return Plan::new(Outcome::EditorSubmit, Row::D1Replay, fx | yield_after);
        }
        let (agent, take_over_fx) = take_over(i, f);
        if agent {
            return drop_plan(DropWhy::AgentControl, Row::D5, fx | take_over_fx);
        }
        return pty_plan(
            i,
            Outcome::Pty(PtyWrite::EnterReplay),
            Row::D11,
            fx | take_over_fx | yield_after,
        );
    }
    // A LIVE terminal Enter takes the keydown path: the control-key predicate, else D10. While the
    // agent owns the pty it is not an agent row: `pty_control` excludes it, so it ends in D10
    // (OD-2: dropped with a log).
    if pty_control(f, false) {
        let (agent, take_over_fx) = take_over(i, f);
        if agent {
            return drop_plan(DropWhy::AgentControl, Row::D5, fx | take_over_fx);
        }
        return pty_plan(
            i,
            Outcome::Pty(PtyWrite::Control),
            Row::D6,
            fx | take_over_fx | yield_after,
        );
    }
    if f.editor_takeover_open && !f.blocks_selected && !f.buffer_empty {
        return Plan::new(Outcome::EditorSubmit, Row::D10, fx | yield_after);
    }
    drop_plan(
        DropWhy::NotLongRunning,
        Row::D10,
        fx | Effects::REFOCUS_GLOBAL | Effects::CLEAR_SELECTION,
    )
}

fn destination_control(i: &Intent, f: &RouteFacts, fx: Effects) -> Plan {
    if pty_control(f, false) {
        let (agent, take_over_fx) = take_over(i, f);
        if agent {
            return drop_plan(DropWhy::AgentControl, Row::D5, fx | take_over_fx);
        }
        return pty_plan(
            i,
            Outcome::Pty(PtyWrite::Control),
            Row::D6,
            fx | take_over_fx,
        );
    }
    if f.editor_takeover_open && !f.blocks_selected {
        return Plan::new(Outcome::EditorAction, Row::D10, fx);
    }
    drop_plan(
        DropWhy::NotLongRunning,
        Row::D10,
        fx | Effects::REFOCUS_GLOBAL | Effects::CLEAR_SELECTION,
    )
}

fn destination_seq(i: &Intent, f: &RouteFacts, fx: Effects) -> Plan {
    // D2b: arrows and friends never reach a command the agent drives.
    if f.agent_owns_pty {
        return drop_plan(DropWhy::AgentControl, Row::D2b, fx);
    }
    if pty_seq(f) {
        let (agent, take_over_fx) = take_over(i, f);
        if agent {
            return drop_plan(DropWhy::AgentControl, Row::D5, fx | take_over_fx);
        }
        return pty_plan(i, Outcome::Pty(PtyWrite::Seq), Row::D6, fx | take_over_fx);
    }
    // Legacy only logs here (no refocus): a sequence cannot form a command line.
    if f.editor_takeover_open && !f.blocks_selected {
        return Plan::new(Outcome::EditorAction, Row::D10, fx);
    }
    drop_plan(DropWhy::NotLongRunning, Row::D10, fx)
}

/// Signals (OD-1: ^C ^D ^Z ^\). They never wait behind a gate.
fn decide_signal(i: &Intent, f: &RouteFacts, signal: SignalKind) -> Plan {
    let abort = matches!(signal, SignalKind::Interrupt | SignalKind::Eof);
    // P1f: a signal held in the ssh segment (HELD_SIGNAL) is delivered to the remote at the
    // hand-over, after the keys typed before it, and never held again.
    if i.origin == Origin::Held {
        return pty_plan(i, Outcome::SignalToPty(signal), Row::P1f, Effects::NONE);
    }
    let keydown = matches!(i.origin, Origin::Keyboard | Origin::Synthesized);
    let mut fx = Effects::NONE;
    if keydown {
        // P1a (OD-5): inside the ssh setup window the signal is HELD with the typed keys and
        // delivered to the REMOTE side at the hand-over, never to the local shell. The window is
        // the one `ssh_interrupt_held` defines; a ^D also waits in the connect window.
        let must_wait =
            f.ssh_interrupt_must_wait || (signal == SignalKind::Eof && f.ssh_eot_must_wait);
        if must_wait {
            return Plan::new(
                Outcome::HoldSignal,
                Row::P1a,
                Effects::NOTE_INTERRUPT_HELD | Effects::NOTE_SSH_HOLDING | Effects::ARM_SSH_GIVE_UP,
            );
        }
        // P1b: the interrupt passes, so what was typed before it is stale (the tty flushes its
        // input on ^C): the ssh hold is reset, then the rows go on.
        if f.ssh_hold_exists {
            fx = fx | Effects::RESET_SSH_HOLD;
        }
        // P1c: a PowerShell setup has its own rules. The AfterAbort machine stays ^C/^D only; the
        // other signals are delivered straight to the leaf, so the PowerShell hold cannot hold
        // them again.
        if f.ps.is_active() {
            return if signal == SignalKind::Interrupt {
                Plan::new(Outcome::PowershellAbort, Row::P1c, fx)
            } else {
                pty_plan(i, Outcome::SignalToPty(signal), Row::P1c, fx)
            };
        }
        // The keydown pty test: a signal that no running command, hand-off or subshell takes has
        // no pty to go to (D10).
        if !pty_control(f, abort) {
            return drop_plan(
                DropWhy::NotLongRunning,
                Row::D10,
                fx | Effects::REFOCUS_GLOBAL | Effects::CLEAR_SELECTION,
            );
        }
        // P1e: ^C while ssh connects.
        if signal == SignalKind::Interrupt {
            if f.ssh_pre_init_abort {
                return pty_plan(i, Outcome::AbortBytes, Row::P1e, fx);
            }
            if f.ssh_sigint_applies {
                return Plan::new(
                    Outcome::SigintForeground,
                    Row::P1e,
                    fx | Effects::LEAVE_USER_WROTE_CTRL_C,
                );
            }
        }
    }
    // P1d: during the paced subshell bootstrap lines every signal is stashed and goes out after
    // the last line (a ^Z in the middle would suspend a half-run script).
    if f.subshell == SubshellPhase::BootstrapWriting {
        return Plan::new(
            Outcome::Stash,
            Row::P1d,
            fx | Effects::CANCEL_PACED_SETUP_WRITES,
        );
    }
    // D4 / D5: the agent still owns the command.
    let (agent, take_over_fx) = take_over(i, f);
    if agent {
        return drop_plan(DropWhy::AgentControl, Row::D5, fx | take_over_fx);
    }
    let fx = fx | take_over_fx | Effects::CANCEL_PACED_SETUP_WRITES;
    // P2: open gates apply their per-signal plan (`signal_plan(gate, signal)`).
    if !f.gates.is_empty() {
        return pty_plan(i, Outcome::SignalAtGates, Row::P2, fx);
    }
    pty_plan(i, Outcome::SignalToPty(signal), Row::D6, fx)
}

/// Line-run pinning: a line that started in the editor stays there when the pty predicate flips
/// mid-line (B5), while the pin is still valid. A pin to the editor is not valid while the hand-off
/// routes to the pty (design: `pin_still_valid` strengthened), nor with an approval card up or the
/// agent owning the pty.
fn apply_pin(plan: Plan, i: &Intent, f: &RouteFacts, pin: Option<Destination>) -> Plan {
    if pin != Some(Destination::Editor) || plan.row != Row::D6 {
        return plan;
    }
    let pin_valid = (f.input_box_visible || f.editor_focused)
        && !f.approval_card
        && !f.agent_owns_pty
        && !f.handoff_routes;
    if !pin_valid {
        return plan;
    }
    let outcome = match (i.class, plan.outcome) {
        (Class::Typed | Class::Paste, Outcome::Pty(PtyWrite::Typed))
        | (Class::Paste, Outcome::PasteToPty { .. }) => Outcome::EditorInsert,
        (Class::Control | Class::Seq, Outcome::Pty(PtyWrite::Control | PtyWrite::Seq)) => {
            Outcome::EditorAction
        }
        _ => return plan,
    };
    Plan {
        outcome,
        effects: Effects(plan.effects.0 & !Effects::CONSULT_ORACLE.0),
        on_refused: OnRefused::NotApplicable,
        row: Row::Pin,
    }
}

// ---- destination_for with the hidden-input branch fixed (B12) ---------------------------------

/// [`super::destination_for`] with the inverted hidden-input branch fixed (B12), for the table's
/// tests and the shadow comparison. The old function stays as it is for its callers.
///
/// The old branch said: not `pty_takes_input` and the input box hidden: drop. Legacy
/// (`after_pty_attempt`) never drops text because the pty does not take it: it inserts into the
/// editor whether or not the box is visible, and drops only when the pty took the decision (it
/// takes input), REFUSED the write, and the box is hidden (D8, which cannot be known before the
/// write: it is [`Plan::on_refused`]).
pub fn destination_for_fixed(kind: &EntryKind, facts: &DestFacts) -> Destination {
    let old = super::destination_for(kind, facts);
    match old {
        Destination::Drop(DropReason::RefusedHiddenInput)
            if !matches!(kind, EntryKind::Raw(_)) && !facts.pty_takes_input =>
        {
            Destination::Editor
        }
        other => other,
    }
}

#[cfg(test)]
#[path = "table_tests.rs"]
mod tests;
