//! Building [`RouteFacts`] (#1424, steps 1 and 2: nothing decides on them yet).
//!
//! The destination table (`input_queue/table.rs`) is pure; this is its only source of facts on the
//! view side. The rules that make it safe to call from anywhere, including events that run outside
//! a key (a give-up cap, the ssh hand-over, a mailbox flush):
//!
//! * It never consults `in_key_path()` and never takes a blocking lock. The terminal model is read
//!   with `try_lock` (the exact state, and the published cells are refreshed from it, as the key
//!   path always did) or, when the pty reader holds it, from a CONSISTENT read of the two
//!   published cells ([`load_consistent`]: the facts and the snapshot of one publication). The
//!   hand-off is read with `try_read`; busy means the last value read.
//! * It reads the legacy ssh / subshell / PowerShell fields as they stand and DERIVES the phases
//!   ([`SshPhase`], [`SubshellPhase`], [`PsPhase`]) from them. It does not run the legacy
//!   refreshers (`refresh_ssh_connect_gate`, `ssh_connect_gate_drops_keys`, ...): those mutate
//!   view state, and they stay on the key path exactly where they are until the edge-driven
//!   machine exists (design step 7). Each has a read-only, non-blocking counterpart here.
//! * It mutates nothing but the idempotent cell refresh above.
//!
//! The tests (`view_test.rs`, `route_facts_*`) hold the model's lock on the test thread while
//! building: a build that waited for it would deadlock, and `model_lock_trace` must not count a
//! key-path lock.

// Steps 1 and 2 of #1424 build facts for the tests only: the table is not called yet.
#![allow(dead_code)]

use std::time::Instant;

use warpui::ViewContext;
use warpui::pane_key::PaneKeyContext;

use super::{
    PowerShellSetupHold, SSH_BOOTSTRAP_SWALLOW_CEILING, SUBSHELL_BOOTSTRAP_STALL_CEILING,
    SUBSHELL_SETUP_TYPED_HOLD_LIMIT, SshBootstrapInterruptRoute, SshConnectGate, TerminalView,
    choice_block_owns_input, ssh_pre_init_abort_bytes, subshell_bootstrap_key_forwarded,
};
use crate::features::FeatureFlag;
use crate::terminal::input_queue::facts::QueueFacts;
use crate::terminal::input_queue::{
    FocusSnap, GateKind, PsPhase, RouteFacts, SshPhase, SubshellPhase,
};
use crate::terminal::model::key_facts::{
    ConsistentRead, KeyFacts, KeyModelSnapshot, load_consistent,
};
use crate::terminal::model::session::SessionId;
use crate::terminal::view::rich_content::RichContentMetadata;

/// [`super::ssh_bootstrap_swallow_active`] without the side effect on the tracked start: whether
/// typing would still be dropped for the pending ssh bootstrap. A session not tracked yet counts
/// as starting now (the legacy function starts the clock on its first call).
fn ssh_bootstrap_swallow_would_be_active(
    pending: Option<SessionId>,
    failed: Option<SessionId>,
    started: Option<(SessionId, Instant)>,
    now: Instant,
) -> bool {
    let Some(sid) = pending else {
        return false;
    };
    if failed == Some(sid) {
        return false;
    }
    match started {
        Some((tracked, at)) if tracked == sid => {
            now.saturating_duration_since(at) < SSH_BOOTSTRAP_SWALLOW_CEILING
        }
        _ => true,
    }
}

impl TerminalView {
    /// What the terminal model says, without waiting for the pty reader and without
    /// `in_key_path()`: the exact state when the model is free (the published cells are refreshed
    /// from it under the gen they already carry), else a consistent read of the published cells.
    /// The non-blocking counterpart of `model_facts()` for everything fact-building needs.
    pub(super) fn model_facts_nonblocking(&self) -> ConsistentRead {
        match self.model.try_lock() {
            Some(model) => {
                let facts = KeyFacts::from_model(&model);
                let snapshot = KeyModelSnapshot::from_model(&model);
                self.key_facts.store(facts);
                self.key_model.store(snapshot.clone());
                ConsistentRead {
                    facts,
                    facts_gen: snapshot.facts_gen,
                    snapshot,
                    consistent: true,
                    exact: true,
                }
            }
            None => load_consistent(&self.key_facts, &self.key_model),
        }
    }

    /// `has_pending_ssh_command` against a snapshot already read (no `is_long_running()`, which
    /// blocks outside the key path).
    fn has_pending_ssh_command_in(&self, snapshot: &KeyModelSnapshot) -> bool {
        self.warpify_state.get_pending_ssh_host().is_some() && snapshot.is_long_running()
    }

    /// `ssh_connect_gate_drops_keys` read-only and non-blocking: the sticky "visible text seen"
    /// comes from the snapshot, the alt screen from the facts. `false` without a gate.
    fn ssh_connect_gate_drops_keys_in(
        &self,
        alt_screen: bool,
        snapshot: &KeyModelSnapshot,
    ) -> bool {
        self.ssh_connect_gate.as_ref().is_some_and(|gate| {
            gate.drops_keys_with(alt_screen, self.gate_output_seen(gate, snapshot))
        })
    }

    /// The gate's `output_seen` as `ssh_connect_gate_drops_keys` would leave it: sticky, set when
    /// the block shows visible text before it ended.
    fn gate_output_seen(&self, gate: &SshConnectGate, snapshot: &KeyModelSnapshot) -> bool {
        gate.output_seen || (!gate.block_done && snapshot.visible_text_seen)
    }

    /// `subshell_bootstrap_announced_age_in` is already pure; the PowerShell hold is read from the
    /// fields (`holds_keys_for_powershell_subshell_setup` against a snapshot already read).
    fn ps_phase_in(&self, snapshot: &KeyModelSnapshot) -> PsPhase {
        match self.powershell_setup_hold {
            PowerShellSetupHold::Off => PsPhase::Off,
            PowerShellSetupHold::LineRunning { .. } => PsPhase::LineRunning,
            PowerShellSetupHold::AfterAbort { .. } => PsPhase::AfterAbort,
            PowerShellSetupHold::Setup
            | PowerShellSetupHold::ScriptRunning
            | PowerShellSetupHold::AwaitingPrompt => PsPhase::Setup {
                holds_keys: !snapshot.read_only
                    && self
                        .subshell_bootstrap_announced
                        .as_ref()
                        .is_some_and(|(_, at)| at.elapsed() < SUBSHELL_SETUP_TYPED_HOLD_LIMIT),
            },
        }
    }

    /// What has focus, from the live focus chain of the key being stamped. The cards are only
    /// looked for when focus is on neither the editor nor the terminal itself (the common cases
    /// answer from the chain alone, so a key does not walk the AI blocks).
    pub(super) fn pane_focus_snap(
        &self,
        key_ctx: &PaneKeyContext,
        ctx: &mut ViewContext<Self>,
    ) -> FocusSnap {
        if key_ctx.in_focus_chain(self.input.as_ref(ctx).editor().id()) {
            return FocusSnap::Editor;
        }
        if key_ctx.is_focused_exactly(self.view_id) {
            return FocusSnap::Terminal;
        }
        if self.ssh_choice_card_holding_keys(ctx).is_some() {
            FocusSnap::SshChoiceCard
        } else if self.typed_on_approval_card(ctx) {
            FocusSnap::ApprovalCard
        } else if self.typed_on_ai_block_awaiting_confirmation(ctx) {
            FocusSnap::AiBlockAwaitingConfirm
        } else if self.focused_settled_ai_target(ctx).is_some() {
            FocusSnap::SettledAiTarget
        } else {
            FocusSnap::Other
        }
    }

    /// The same, from the window's current focus (no key context): for facts built outside a key.
    fn live_focus_snap(&self, ctx: &mut ViewContext<Self>) -> FocusSnap {
        if self.ssh_choice_card_holding_keys(ctx).is_some() {
            FocusSnap::SshChoiceCard
        } else if self.typed_on_approval_card(ctx) {
            FocusSnap::ApprovalCard
        } else if self.typed_on_ai_block_awaiting_confirmation(ctx) {
            FocusSnap::AiBlockAwaitingConfirm
        } else if self.focused_settled_ai_target(ctx).is_some() {
            FocusSnap::SettledAiTarget
        } else if self.input.as_ref(ctx).editor().is_focused(ctx) {
            FocusSnap::Editor
        } else if ctx.focused_view_id(ctx.window_id()) == Some(self.view_id) {
            FocusSnap::Terminal
        } else {
            FocusSnap::Other
        }
    }

    /// The facts one destination decision is made on (#1424). `queue` is what the queue says
    /// about itself: the caller is either the queue (mid-drain, where `try_peek` would fail) or
    /// the sink (which peeks). Never blocks, never reads `in_key_path()`, never runs a refresher
    /// that mutates view state; see the module docs.
    pub(super) fn build_route_facts(
        &self,
        queue: QueueFacts,
        ctx: &mut ViewContext<Self>,
    ) -> RouteFacts {
        let route_state = self.paste_queue.queue.route_state();
        let model = self.model_facts_nonblocking();
        let snapshot = &model.snapshot;
        let now = Instant::now();

        // ---- the typeahead hand-off (try_read: busy means the last value read) ----
        let (handoff_routes, handoff_known, editor_takeover_open) =
            if FeatureFlag::TypeaheadHandoff.is_enabled() {
                match self.handoff.try_read() {
                    Some(handoff) => {
                        let routes = handoff.routes_keys_to_pty();
                        route_state.set_handoff_routes(routes);
                        (routes, true, handoff.editor_takeover_would_be_open())
                    }
                    None => (route_state.handoff_routes(), false, false),
                }
            } else {
                (false, true, false)
            };

        // ---- subshell / PowerShell ----
        let subshell_pending = !snapshot.read_only
            && snapshot.has_pending_subshell
            && !snapshot.active_block_bootstrapped;
        let announced_age = self.subshell_bootstrap_announced_age_in(snapshot);
        let subshell = if queue.gates.has(GateKind::SubshellBootstrapWrite) {
            SubshellPhase::BootstrapWriting
        } else if queue.gates.has(GateKind::SubshellSetup) {
            SubshellPhase::SetupHeld
        } else if self.subshell_bootstrap_announced.is_some() {
            SubshellPhase::Announced
        } else {
            SubshellPhase::Idle
        };
        let ps = self.ps_phase_in(snapshot);

        // ---- ssh ----
        let alt_screen = model.facts.alt_screen;
        let pending_now = snapshot.pending_session_id;
        let has_pending_ssh_command = self.has_pending_ssh_command_in(snapshot);
        let gate = self.ssh_connect_gate.as_ref();
        let drops_keys = self.ssh_connect_gate_drops_keys_in(alt_screen, snapshot);
        let pending_sid = if gate.is_some() || has_pending_ssh_command {
            pending_now
        } else {
            None
        };
        let ssh_bootstrapping = ssh_bootstrap_swallow_would_be_active(
            pending_sid,
            self.ssh_bootstrap_failed_session,
            self.ssh_bootstrap_swallow_started,
            now,
        );
        let ssh_connecting_silently = pending_now.is_none() && drops_keys;
        let ssh_dialog_up = choice_block_owns_input(
            self.rich_content_views
                .iter()
                .filter_map(|view| match view.metadata() {
                    Some(RichContentMetadata::SshRemoteServerChoiceBlock { handle }) => {
                        Some(handle.as_ref(ctx).session_id())
                    }
                    _ => None,
                }),
            pending_now,
        );
        let ssh_setup_in_flight = FeatureFlag::SshRemoteServer.is_enabled()
            && pending_now.is_some_and(|sid| {
                self.sessions
                    .as_ref(ctx)
                    .remote_server_setup_state(sid)
                    .is_some_and(|state| state.is_in_progress())
            });
        let route = self.ssh_bootstrap_interrupt_route_in(snapshot);
        let unterminated = snapshot.unterminated_since_gen;
        let setup_echo_mid_line = self.setup_echo_mid_line_at_last_look.get();
        let awaits_an_answer = unterminated.is_some() && !setup_echo_mid_line;
        // `ssh_gate_holds_interrupt`: the remote-setup window only.
        let ssh_interrupt_must_wait = route == Some(SshBootstrapInterruptRoute::Hold)
            || (pending_now.is_none() && gate.is_some_and(|g| g.block_done));
        // `ssh_connect_holds_eot`: the same, and a ^D also waits in the unusable connect window.
        let ssh_eot_must_wait = gate.is_some()
            && (ssh_interrupt_must_wait
                || (pending_now.is_none()
                    && (snapshot.has_pending_ssh_session || drops_keys || !awaits_an_answer)));
        // `ssh_prompt_has_arrived`: visible, unterminated text on an unmanaged connect.
        let prompt_up = !drops_keys
            && gate.is_some_and(|g| {
                self.gate_output_seen(g, snapshot) && !g.block_done && !g.managed && !g.saw_pending
            });
        let ssh = match unterminated {
            Some(prompt_gen) if prompt_up => SshPhase::PromptUp { prompt_gen },
            _ if self.ssh_typeahead_holding => SshPhase::Holding,
            _ => SshPhase::Idle,
        };

        // ---- focus, cards, editor ----
        let focus = self.live_focus_snap(ctx);
        let approval_card = focus == FocusSnap::ApprovalCard;
        let ai_awaiting_confirmation = focus == FocusSnap::AiBlockAwaitingConfirm;
        let setup_gate_open = queue.gates.has(GateKind::SubshellSetup)
            || queue.gates.has(GateKind::SubshellBootstrapWrite)
            || queue.gates.has(GateKind::PowershellSetup);

        let facts = RouteFacts {
            facts_gen: model.facts_gen,
            model_consistent: model.consistent,
            alt_screen,
            agent_in_control: model.facts.agent_in_control,
            agent_in_control_exact: model.exact,
            agent_owns_pty: snapshot.agent_owns_pty,
            long_running: snapshot.is_long_running(),
            read_only: snapshot.read_only,
            is_bootstrapped: snapshot.is_bootstrapped,
            needs_bracketed_paste: snapshot.needs_bracketed_paste,
            active_block_started: snapshot.active_block_started,
            shared_session_executor: snapshot.shared_session_executor,
            session_pending_or_bootstrapped: self
                .sessions
                .as_ref(ctx)
                .has_pending_or_bootstrapped_session(),
            has_pending_ssh_command,
            handoff_routes,
            handoff_known,
            subshell,
            forwards_subshell_key: subshell_bootstrap_key_forwarded(
                subshell_pending,
                false,
                announced_age,
            ),
            forwards_subshell_abort: subshell_bootstrap_key_forwarded(
                subshell_pending,
                true,
                announced_age,
            ),
            subshell_awaiting_answer: announced_age.is_some()
                && self.subshell_bootstrap_in_flight.is_none()
                && awaits_an_answer,
            subshell_bootstrap_stalled: announced_age
                .is_some_and(|age| age >= SUBSHELL_BOOTSTRAP_STALL_CEILING),
            ps,
            ssh,
            ssh_dialog_up,
            ssh_setup_in_flight,
            ssh_bootstrapping,
            ssh_connecting_silently,
            ssh_interrupt_must_wait,
            ssh_eot_must_wait,
            ssh_hold_exists: self.ssh_typeahead_holding || self.paste_queue.queue.ssh_hold_active(),
            queue_has_ssh_gate: queue.gates.has(GateKind::SshConnect),
            ssh_route_forward: route == Some(SshBootstrapInterruptRoute::Forward),
            ssh_pre_init_abort: ssh_pre_init_abort_bytes(
                snapshot.has_pending_ssh_session,
                pending_now.is_some(),
            )
            .is_some(),
            ssh_sigint_applies: !snapshot.has_pending_ssh_session
                && pending_now.is_none()
                && gate.is_some_and(|g| !g.block_done),
            focus,
            approval_card,
            ai_awaiting_confirmation,
            settled_ai_target: focus == FocusSnap::SettledAiTarget,
            input_box_visible: self.is_input_box_visible_with(&snapshot.input_box, ctx),
            editor_focused: self.input.as_ref(ctx).editor().is_focused(ctx),
            editor_takeover_open,
            blocks_selected: !self.selected_blocks.is_empty(),
            buffer_empty: self.input.as_ref(ctx).buffer_text(ctx).is_empty(),
            cli_agent_session: self.has_active_cli_agent_input_session(ctx),
            gates: queue.gates,
            queued: queue.queued,
            queued_only_pty: queue.queued_only_pty,
            mailbox_non_empty: self.paste_queue.queue.has_deferred(),
            agent_write_gate_closed: setup_gate_open
                || self.subshell_bootstrap_announced.is_some()
                || self.subshell_bootstrap_in_flight.is_some()
                || ps.is_active()
                || ssh_interrupt_must_wait,
        };
        // The phases are also published to the pane, for readers that cannot build facts (the
        // editor's producers).
        route_state.set_ssh(facts.ssh);
        route_state.set_subshell(facts.subshell);
        route_state.set_ps(facts.ps);
        facts
    }
}
