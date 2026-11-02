//! The plain data the destination table (`table.rs`, #1424) decides on, and where it lives.
//!
//! Nothing here decides anything. The shapes are:
//!
//! * [`RouteFacts`]: one `Copy` value per decision. The view builds it (`TerminalView::
//!   build_route_facts`, `view/route_facts.rs`) from three sources: a consistent read of the two
//!   published model cells (no model lock), the [`PaneRouteState`] the view writes at event edges,
//!   and live reads of view state. Building it never takes a blocking lock and never consults
//!   `in_key_path()`: the 2 ms budget covers EVERY decision, including the ones an event (a
//!   give-up cap, the ssh hand-over, a mailbox flush) triggers outside a key.
//! * [`PaneRouteState`]: `Cell`s in `PaneQueue::Inner`, next to `ssh_hold_active`, written by the
//!   view and read by anyone (the editor's deferred offers stamp from it).
//! * [`Stamp`]: what was true when a key reached the pane's sink. It travels with the entry
//!   (`InputEntry::stamp`, `Deferred::Offer`), so a key that waited in the mailbox keeps the
//!   classification it had when the user pressed it (the password rule, H4).
//! * [`SshPhase`], [`SubshellPhase`], [`PsPhase`]: DERIVED, read-only, from the legacy fields after
//!   the legacy refreshers ran (`refresh_ssh_connect_gate` and friends stay on the key path until
//!   the edge-driven machine exists, step 7 of the design).
//!
//! `PHOSPHOR_QUEUE_DESTINATION=legacy|shadow|table` ([`DestinationMode`]) is the kill switch.
//! The default is `legacy`; as of steps 1 and 2 nothing reads `shadow` or `table`.

use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

use super::GateKind;
use crate::terminal::model::key_facts::{KeyFactsCell, KeyModelCell, load_gen_consistent};

/// Which destination logic is in charge (`PHOSPHOR_QUEUE_DESTINATION`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DestinationMode {
    /// The legacy chain decides (the default; the table is not consulted).
    #[default]
    Legacy,
    /// The table decides next to the legacy chain and the two are compared in the log. Not read
    /// by anything yet.
    Shadow,
    /// The table decides. Not read by anything yet.
    Table,
}

impl DestinationMode {
    /// Parsed like `PHOSPHOR_PANE_KEY` (`pane_key::PaneKeyMode::parse`): `shadow` and `table`
    /// select those modes; `legacy`, `0`, `off`, empty, unset and anything unknown are `Legacy`
    /// (an unknown value must never switch the destination logic).
    pub fn parse(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some("shadow") => Self::Shadow,
            Some("table") => Self::Table,
            _ => Self::Legacy,
        }
    }
}

/// The mode of this process, read once.
pub fn destination_mode() -> DestinationMode {
    static MODE: OnceLock<DestinationMode> = OnceLock::new();
    *MODE.get_or_init(|| {
        DestinationMode::parse(std::env::var("PHOSPHOR_QUEUE_DESTINATION").ok().as_deref())
    })
}

/// What had focus when a key reached the sink (the stamped focus chain, design §2.6): the focus
/// rows (H1, D1, D9) use THIS, not the focus of the moment the entry is drained.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FocusSnap {
    /// Nothing the table needs to tell apart (a gap between views, a text field of the pane).
    #[default]
    Other,
    /// The terminal view itself.
    Terminal,
    /// The input editor.
    Editor,
    /// The ssh remote-server choice card (the input is hidden, keys only show its hint).
    SshChoiceCard,
    /// The pending approval card.
    ApprovalCard,
    /// An AI block awaiting confirmation.
    AiBlockAwaitingConfirm,
    /// A settled AI block or action card.
    SettledAiTarget,
}

/// What was true when the key reached the pane's sink (`on_pane_key_event`), or at submit for an
/// origin without a sink. Plain data, created BEFORE any `try_lock` can fail, so a key that goes
/// through the deferred mailbox keeps it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stamp {
    /// The publication of the model facts the key saw ([`crate::terminal::model::key_facts::
    /// KeyModelSnapshot::facts_gen`]); 0 if the key was stamped before any publication.
    pub facts_gen: u64,
    /// The focus chain at the sink.
    pub focus: FocusSnap,
    /// The queue's focus epoch.
    pub epoch: u64,
}

impl Stamp {
    /// Not stamped by a sink (tests, internal entries): gen 0, so it is "before" every prompt.
    pub const UNSTAMPED: Stamp = Stamp {
        facts_gen: 0,
        focus: FocusSnap::Other,
        epoch: 0,
    };

    /// The key was stamped before publication `prompt_gen`: it was typed before the prompt
    /// existed, so it may be a password typed blind (H4). Strictly before: a key stamped AT the
    /// prompt's gen saw the prompt in its facts.
    pub fn is_before(&self, prompt_gen: u64) -> bool {
        self.facts_gen < prompt_gen
    }
}

/// A set of open gate kinds, one bit per [`GateKind`] (the queue's `gate_mask`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GateMask(pub u16);

impl GateMask {
    pub const EMPTY: GateMask = GateMask(0);

    fn bit(kind: GateKind) -> u16 {
        1 << (GateKind::ALL.iter().position(|k| *k == kind).unwrap_or(0) as u16)
    }

    pub fn of(kinds: &[GateKind]) -> Self {
        Self(kinds.iter().fold(0, |mask, kind| mask | Self::bit(*kind)))
    }

    pub fn with(self, kind: GateKind) -> Self {
        Self(self.0 | Self::bit(kind))
    }

    pub fn has(self, kind: GateKind) -> bool {
        self.0 & Self::bit(kind) != 0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Every open gate is one of `kinds` (and at least one is open).
    pub fn all_within(self, kinds: &[GateKind]) -> bool {
        !self.is_empty() && self.0 & !Self::of(kinds).0 == 0
    }

    pub fn kinds(self) -> impl Iterator<Item = GateKind> {
        GateKind::ALL
            .into_iter()
            .filter(move |kind| self.has(*kind))
    }
}

/// Where the ssh connect typeahead stands. DERIVED from the legacy fields (`ssh_typeahead_holding`,
/// the connect gate, the prompt test), never set by an event yet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SshPhase {
    /// No ssh connect holds keys.
    #[default]
    Idle,
    /// Keys are being held until the remote shell shows its first prompt.
    Holding,
    /// A host-key / password prompt is up (unterminated text before any remote session), first
    /// published at `prompt_gen`. A key stamped before it may be a password typed blind (H4).
    PromptUp { prompt_gen: u64 },
}

/// The subshell (`su -`, `sudo -i`) setup, derived from the open gates and the announcement.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SubshellPhase {
    #[default]
    Idle,
    /// Announced by the shell and awaiting its answer; no setup gate is open yet.
    Announced,
    /// The setup gate holds the keys (`GateKind::SubshellSetup`).
    SetupHeld,
    /// The paced bootstrap lines are being written (`GateKind::SubshellBootstrapWrite`).
    BootstrapWriting,
}

/// The PowerShell subshell setup (`powershell_setup_hold`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PsPhase {
    #[default]
    Off,
    /// Setup, script running or awaiting the prompt. `holds_keys` is false once the typed-hold
    /// limit passed or the session is read only (`holds_keys_for_powershell_subshell_setup`).
    Setup {
        holds_keys: bool,
    },
    LineRunning,
    AfterAbort,
}

impl PsPhase {
    /// Every non-abort key joins the held keys (H6).
    pub fn holds_keys(self) -> bool {
        match self {
            PsPhase::Off => false,
            PsPhase::Setup { holds_keys } => holds_keys,
            PsPhase::LineRunning | PsPhase::AfterAbort => true,
        }
    }

    pub fn is_active(self) -> bool {
        self != PsPhase::Off
    }
}

/// Everything the table decides on, as plain data. One value per decision. See the module docs
/// for how it is built. `Default` is "nothing is true": an idle shell prompt with no input box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RouteFacts {
    // ---- the model (a consistent read of the two published cells) ----
    /// The publication these facts belong to.
    pub facts_gen: u64,
    /// The two published cells agreed on one gen. False: the facts are one publication behind.
    pub model_consistent: bool,
    pub alt_screen: bool,
    pub agent_in_control: bool,
    /// `agent_in_control` was read from the exact model (it was free), not the published copy.
    /// The key path used to take a blocking lock when the copy said true; a decision never does.
    pub agent_in_control_exact: bool,
    /// The agent, not the user, owns the running command's pty (snapshot).
    pub agent_owns_pty: bool,
    pub long_running: bool,
    pub read_only: bool,
    pub is_bootstrapped: bool,
    pub needs_bracketed_paste: bool,
    pub active_block_started: bool,
    pub shared_session_executor: bool,
    /// The sessions model has a pending or bootstrapped session (the bootstrap script echoed).
    pub session_pending_or_bootstrapped: bool,
    /// `warpify_state.get_pending_ssh_host().is_some() && long_running`.
    pub has_pending_ssh_command: bool,

    // ---- the typeahead hand-off ----
    /// The hand-off routes keys to the pty (`routes_keys_to_pty`). If the hand-off lock was busy
    /// the last known value is used (`handoff_known` false).
    pub handoff_routes: bool,
    pub handoff_known: bool,

    // ---- subshell / PowerShell ----
    pub subshell: SubshellPhase,
    /// `forwards_keys_during_subshell_bootstrap(false)` / `(true)`: only the abort keys are
    /// forwarded during the bootstrap.
    pub forwards_subshell_key: bool,
    pub forwards_subshell_abort: bool,
    /// A subshell setup was announced and the shell is asking for something (a prompt waiting for
    /// an answer): typing is the answer, so the subshell holds let keys through (H7b).
    pub subshell_awaiting_answer: bool,
    /// The announced subshell bootstrap is older than `SUBSHELL_BOOTSTRAP_STALL_CEILING`,
    /// computed when the facts are built so the table reads no clock.
    pub subshell_bootstrap_stalled: bool,
    pub ps: PsPhase,

    // ---- ssh ----
    pub ssh: SshPhase,
    /// The setup dialog (choice block) owns the keyboard for the pending session.
    pub ssh_dialog_up: bool,
    pub ssh_setup_in_flight: bool,
    pub ssh_bootstrapping: bool,
    pub ssh_connecting_silently: bool,
    /// An interrupt must not reach the pty now: the remote bootstrap route is Hold, or the
    /// remote-setup window, or ^D in the connect window (`ssh_interrupt_held`).
    pub ssh_interrupt_must_wait: bool,
    /// A ^D (EOT) must also wait: the connect window before the remote shell is usable
    /// (`ssh_connect_holds_eot`). Implies nothing about the other three signals.
    pub ssh_eot_must_wait: bool,
    /// The ssh hold exists (keys are held, or the view holds state for them).
    pub ssh_hold_exists: bool,
    /// An `SshConnect` gate is open in the queue (it may be stale: the detach window).
    pub queue_has_ssh_gate: bool,
    /// `ssh_bootstrap_interrupt_route() == Some(Forward)`.
    pub ssh_route_forward: bool,
    /// `~.` would abort the connect (`ssh_pre_init_abort_bytes` is `Some`).
    pub ssh_pre_init_abort: bool,
    /// A ^C applies as SIGINT to the foreground group (`ssh_connect_sigint_applies`).
    pub ssh_sigint_applies: bool,

    // ---- focus, cards, editor ----
    pub focus: FocusSnap,
    pub approval_card: bool,
    pub ai_awaiting_confirmation: bool,
    pub settled_ai_target: bool,
    pub input_box_visible: bool,
    pub editor_focused: bool,
    /// The editor took input over from the pty (`editor_takeover_open`).
    pub editor_takeover_open: bool,
    pub blocks_selected: bool,
    pub buffer_empty: bool,
    pub cli_agent_session: bool,

    // ---- the queue ----
    pub gates: GateMask,
    /// Entries are queued (anything at all).
    pub queued: bool,
    /// Every queued entry is pty-bound bytes (the second half of `editor_keys_pass_gates`).
    pub queued_only_pty: bool,
    /// The deferred mailbox is not empty: earlier operations wait, a new arrival must not
    /// overtake them.
    pub mailbox_non_empty: bool,
    /// `agent_pty_write_gate`: a setup gate or the ssh remote setup refuses agent writes.
    pub agent_write_gate_closed: bool,
}

/// What the queue itself contributes to a decision (the host cannot peek at the queue mid-drain,
/// so whoever calls `build_route_facts` hands this in: the queue from inside a drain, the sink from
/// `try_peek`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QueueFacts {
    pub gates: GateMask,
    /// Entries are queued (anything at all).
    pub queued: bool,
    /// Every queued entry is pty-bound bytes (the second half of `editor_keys_pass_gates`).
    pub queued_only_pty: bool,
}

/// The pane-level state the view writes at event edges and the editor reads when it stamps a
/// deferred offer: `Cell`s next to `ssh_hold_active` in `PaneQueue::Inner`. Everything runs on the
/// main thread (`PaneQueue` is an `Rc`), so cells suffice and nothing here locks.
#[derive(Debug, Default)]
pub struct PaneRouteState {
    /// The two published model cells (shared `Arc`s, set once by the view). A stamp reads the gen
    /// they agree on at the moment it is taken, so a producer that never passed the sink (the
    /// editor's own key bindings) is stamped fresh, not with the last sink key's gen.
    gen_source: RefCell<Option<(KeyFactsCell, KeyModelCell)>>,
    facts_gen: Cell<u64>,
    focus: Cell<FocusSnap>,
    epoch: Cell<u64>,
    ssh: Cell<SshPhase>,
    subshell: Cell<SubshellPhase>,
    ps: Cell<PsPhase>,
    /// The last hand-off `routes_keys_to_pty` the view read.
    handoff_routes: Cell<bool>,
}

impl PaneRouteState {
    /// The view hands the pane the cells the model publishes to.
    pub fn set_gen_source(&self, facts: KeyFactsCell, model: KeyModelCell) {
        *self.gen_source.borrow_mut() = Some((facts, model));
    }

    /// The publication gen now: the gen the published cells agree on, else the last one noted.
    /// Takes no model lock (two atomic loads and the snapshot cell's leaf mutex).
    pub fn current_facts_gen(&self) -> u64 {
        match self.gen_source.borrow().as_ref() {
            Some((facts, model)) => {
                let (generation, _consistent) = load_gen_consistent(facts, model);
                self.facts_gen.set(generation);
                generation
            }
            None => self.facts_gen.get(),
        }
    }

    /// A stamp: the gen now, the latest noted focus and epoch.
    pub fn stamp(&self) -> Stamp {
        Stamp {
            facts_gen: self.current_facts_gen(),
            focus: self.focus.get(),
            epoch: self.epoch.get(),
        }
    }

    /// The sink read the published facts at `facts_gen`. Stored as read: never a maximum (a
    /// main-thread refresh stores the gen it read from the model without changing it).
    pub fn note_facts_gen(&self, facts_gen: u64) {
        self.facts_gen.set(facts_gen);
    }

    pub fn facts_gen(&self) -> u64 {
        self.facts_gen.get()
    }

    pub fn note_focus(&self, focus: FocusSnap) {
        self.focus.set(focus);
    }

    pub fn focus(&self) -> FocusSnap {
        self.focus.get()
    }

    /// The queue's focus epoch (mirrored by the guard, which is the only thing that changes it).
    pub fn note_epoch(&self, epoch: u64) {
        self.epoch.set(epoch);
    }

    pub fn epoch(&self) -> u64 {
        self.epoch.get()
    }

    pub fn set_ssh(&self, phase: SshPhase) {
        self.ssh.set(phase);
    }

    pub fn ssh(&self) -> SshPhase {
        self.ssh.get()
    }

    pub fn set_subshell(&self, phase: SubshellPhase) {
        self.subshell.set(phase);
    }

    pub fn subshell(&self) -> SubshellPhase {
        self.subshell.get()
    }

    pub fn set_ps(&self, phase: PsPhase) {
        self.ps.set(phase);
    }

    pub fn ps(&self) -> PsPhase {
        self.ps.get()
    }

    /// The last hand-off `routes_keys_to_pty` the view read (the value used when the hand-off
    /// lock is busy).
    pub fn set_handoff_routes(&self, routes: bool) {
        self.handoff_routes.set(routes);
    }

    pub fn handoff_routes(&self) -> bool {
        self.handoff_routes.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_parses_like_the_pane_key_switch_and_defaults_to_legacy() {
        assert_eq!(DestinationMode::parse(None), DestinationMode::Legacy);
        for legacy in ["", "legacy", "0", "off", "garbage", " LEGACY "] {
            assert_eq!(
                DestinationMode::parse(Some(legacy)),
                DestinationMode::Legacy,
                "{legacy:?}"
            );
        }
        assert_eq!(
            DestinationMode::parse(Some("shadow")),
            DestinationMode::Shadow
        );
        assert_eq!(
            DestinationMode::parse(Some(" table ")),
            DestinationMode::Table
        );
    }

    #[test]
    fn a_stamp_is_before_a_prompt_only_when_strictly_older() {
        let stamp = Stamp {
            facts_gen: 5,
            ..Stamp::UNSTAMPED
        };
        assert!(stamp.is_before(6));
        assert!(
            !stamp.is_before(5),
            "a key stamped at the prompt's gen saw it"
        );
        assert!(!stamp.is_before(4));
    }

    #[test]
    fn gate_masks_hold_every_kind_independently() {
        let mut seen = GateMask::EMPTY;
        for kind in GateKind::ALL {
            assert!(!seen.has(kind));
            seen = seen.with(kind);
            assert!(seen.has(kind));
            assert_eq!(
                GateMask::of(&[kind]).kinds().collect::<Vec<_>>(),
                vec![kind]
            );
        }
        assert_eq!(seen.kinds().count(), GateKind::ALL.len());
        let setup = GateMask::of(&[GateKind::SubshellSetup, GateKind::PowershellSetup]);
        assert!(setup.all_within(&[GateKind::SubshellSetup, GateKind::PowershellSetup]));
        assert!(
            !setup
                .with(GateKind::SshConnect)
                .all_within(&[GateKind::SubshellSetup, GateKind::PowershellSetup])
        );
        assert!(!GateMask::EMPTY.all_within(&[GateKind::SshConnect]));
    }

    #[test]
    fn route_state_stamps_what_was_noted() {
        let state = PaneRouteState::default();
        assert_eq!(state.stamp(), Stamp::UNSTAMPED);
        state.note_facts_gen(9);
        state.note_focus(FocusSnap::Editor);
        state.note_epoch(3);
        assert_eq!(
            state.stamp(),
            Stamp {
                facts_gen: 9,
                focus: FocusSnap::Editor,
                epoch: 3
            }
        );
    }
}
