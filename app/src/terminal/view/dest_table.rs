//! The terminal view's side of the destination table (#1424).
//!
//! **Step 3, the SHADOW hooks.** In `PHOSPHOR_QUEUE_DESTINATION=shadow` the legacy entry points
//! (`typed_characters_on_terminal`, `keydown_on_terminal`, `control_sequence_on_terminal`, the
//! replayed Enter) are wrapped: [`TerminalView::shadow_begin`] builds the facts and asks the table
//! what it would decide, BEFORE the legacy chain touches any state; the legacy chain runs
//! unchanged and notes the destination class it reached at its leaf (`shadow::note_leaf`);
//! [`TerminalView::shadow_end`] compares the two and logs a disagreement (`kh_dest_disagree`).
//! Nothing there changes where an input goes.
//!
//! **Step 4, delivery through the table (the #1415 fix).** In `PHOSPHOR_QUEUE_DESTINATION=table`
//! the queue's delivery of text and Enters typed behind an Enter that waited, and of every input
//! the ssh connect typeahead held (`EntryKind::Held`), stops calling the legacy chain
//! (`typed_characters_on_terminal` -> `route_typed` -> the hold probes -> `write_user_bytes_to_pty`
//! ..., which could hold the input AGAIN from inside the delivery and recover through the
//! `delivering` / `requeue_request` plumbing). [`TerminalView::deliver_via_table`] builds the
//! facts of THIS moment (since step 7 the ssh refreshers are gone from the key path: the edge-driven
//! `SshPhase` of `view/ssh_edges.rs` carries the connect state, and a prompt that appeared
//! discards what was held), decides ONCE with `table::decide` (the entry's own
//! stamp, no gate behind it: the queue delivered it, so none covers it) and executes the plan with
//! the same leaf writers legacy ends in. A plan that holds the input behind the ssh connect
//! requeues it exactly as `ssh_hold_push` always did mid-drain.
//!
//! **Step 5, arrival of terminal keys** (the section below `Arrival`): in `table` mode a live key
//! is decided by the table at the three entry points, the idle path included; the origin rows of
//! the writers of other origins (synced input, the paced CLI-agent submit, the viewer) and the H8
//! editor column are wired.
//!
//! **Step 6, paste, the agent's writes and the audit of the pty write call sites (R11).** A
//! terminal paste is routed by the destination rows (`paste_routing_via_table`); the agent's
//! write consults row D13 at its writer; the navigation keys of the key bindings are compared in
//! `shadow`. Every site that puts user-originated bytes on the pty, and what became of it:
//!
//! | site | status | why |
//! |---|---|---|
//! | `typed_characters_on_terminal`, `keydown_on_terminal`, `control_sequence_on_terminal` | ROUTED (table mode: `arrive_via_table`; shadow: compared at the leaves) | live keys |
//! | `replay_held_key`, `route_after_enter_input` (replayed Enter), `deliver_ssh_held`, `deliver_paste_text` | ROUTED (step 4, `deliver_via_table`) | deliveries |
//! | `paste` (the routing decision) | ROUTED (table mode: `paste_routing_via_table`; shadow: compared) | a paste read takes its own `PasteReading` gate; the table decides the destination only |
//! | `write_to_pty_for_syncing_long_running_commands` | ROUTED (origin row `O-synced`, judged as synced input) | text goes through the arrival, raw bytes through `begin_origin_row` |
//! | `write_cli_submit_bytes` (9 callers in `use_agent_footer`) | WIRED (origin row `O-cli-submit`, exempt from the holds; the table's plan is logged, shadow compares) | the paced submit owns a `CliSubmit` gate and writes into the stage |
//! | `write_viewer_bytes_to_pty` | WIRED (origin `Viewer`; test-only writer in this tree) | no production caller |
//! | `write_agent_bytes_to_pty` | ROUTED (row `D13`, refusal) | refused while a setup owns the pty, as `agent_pty_write_gate` |
//! | `terminal_up` / `terminal_down` / `page_up` / `page_down` / `move_home` / `move_end` | NOT ROUTED, compared in shadow (`nav_key_goes_to_pty`) | their fall-back is a scroll or a block-selection move, which no row models, and their predicates differ by design (B11); `terminal_up` writes passively |
//! | `alt_scroll`, `alt_mouse_action` (`write_passive_input_to_pty`) | NOT ROUTED | pointer-driven at wheel / mouse rate: building facts per event breaks the 2 ms budget; the only rule (`D3`, drop when the agent is in control) is the stage's own |
//! | `user_write_ctrl_c_to_pty`, `ctrl_d`, `InputEvent::CtrlD` (and `Input::ctrl_d`'s `Event::CtrlD`) | NOT ROUTED | explicit Stop / Escape / Ctrl-C / Ctrl-D actions with their own marks (`mark_interrupted_by_ctrl_c`, `ignore_bootstrapping_messages`); they write through the queue's writer and ask the same ssh facts (`ssh_connect_holds_eot`); the signal rows are the keyboard's |
//! | `try_send_text_to_cli_agent_or_rich_input`, `use_agent_footer` (2 text writes) | NOT ROUTED | CLI-agent plumbing that always means the pty; through the queue's writer (#1433) |
//! | `deliver_powershell_held_bytes`, `release_held_keys`, `apply_young_hold_end`, `release_young_hold_keys`, `release_queued_prompt_hold` | NOT ROUTED | take-and-write: the hand-off's and the PowerShell gate's keys come back as BYTES, which the design keeps (R3; step 8 retires them with the legacy fields) |
//! | `write_init_subshell_bytes_to_pty`, `write_subshell_bootstrap_line`, `clear_line_editor_and_write_to_pty_with_mac_workaround_hack` (the ssh warpify payloads), `ctrl_c_during_powershell_setup` (raw ETX) | NOT ROUTED (allowlisted doors) | app-generated setup bytes that OWN the setup gates, or must reach the console ahead of the PowerShell gate's keys |
//! | `maybe_report_focus_in/out`, the OSC 52 reply | NOT ROUTED (allowlisted doors) | terminal protocol answers, never user text |
//! | the editor's Enter submit (`PtyIntent::ExecuteCommand`) and the nested write of a delivery (#1418) | NOT ROUTED | the nested write needs follow-ups placed at the delivered entry's position (`EntryId` as a `(major, minor)` pair, R11), which touches every barrier comparison: step 8 |
//! | the editor's own keys (`hold_key_for_paste_on_queue`) | PARTLY ROUTED | the H8 matrix decides whether a key waits behind the gates (`editor_key_passes_gates_by_table`); its ssh, Enter-verdict and display-map holds keep their producers |
//! | `write_user_bytes_to_pty_inner`, `_after_paste_stage`, `write_to_pty`, `PtyQueueHost::deliver_entry` | the writers themselves | what the table's plans execute into |
//!
//! The "open doors" issues #1429-#1437 were closed with the queue / command path and the
//! `script/check_pty_doors` allowlist; nothing in this table reopens one.
//!
//! Never logged: the text of an input (it can be a password). Rows, classes and counts only.

use std::borrow::Cow;
use std::cell::Cell;

use warpui::{TypedActionView as _, ViewContext};

use super::paste_queue::{PtyQueueHost, signal_bytes};
use super::ssh_typeahead::HeldInput;
use super::{AfterEnterInput, Event, KeyPathScope, SshKeyEvent, TerminalView, paste_bytes_for_pty};
use crate::editor::EditorAction;
use crate::features::FeatureFlag;
use crate::terminal::block_list_viewport::ScrollPositionUpdate;
use crate::terminal::input_queue::shadow::{self, DestClass};
use crate::terminal::input_queue::table::{
    self, Class, DropWhy, Effects, Intent, OnRefused, Outcome, Plan, Row,
};
use crate::terminal::input_queue::{
    Deferred, DestinationMode, EntryKind, GateKind, HELD_KEY, HELD_PASTE, HELD_SEQ, HELD_SIGNAL,
    HELD_TYPED, Origin, QueueFacts, RouteFacts, SignalKind, SshPhase, Stamp, destination_mode,
};

/// What the table decided at the top of a legacy entry point, kept until its leaf is known.
pub(super) struct ShadowPending {
    intent: Intent,
    plan: Plan,
    facts: RouteFacts,
}

impl TerminalView {
    /// The origin the input at a legacy entry point is judged with: the delivering entry's
    /// (replayed text, an ssh-held input) while the queue delivers, else a key from the keyboard.
    pub(super) fn shadow_origin(&self) -> Origin {
        if self.paste_queue.delivering {
            self.paste_queue.delivering_origin
        } else {
            arrival_origin()
        }
    }

    /// Starts a legacy entry point: forgets the previous leaf and, when the shadow comparison is
    /// wanted, decides with the table on the facts and the stamp of this moment. Builds facts
    /// without blocking and without mutating view state (`build_route_facts`).
    pub(super) fn shadow_begin(
        &self,
        class: Class,
        ctx: &mut ViewContext<Self>,
    ) -> Option<ShadowPending> {
        shadow::begin_leaf();
        if !shadow::wanted() {
            return None;
        }
        let delivering = self.paste_queue.delivering;
        // A delivered entry is judged by the stamp it got at its sink and without the gates behind
        // it (the queue delivered it, so none covers it); a live key by the stamp of now.
        let (stamp, queue_facts) = if delivering {
            (self.paste_queue.delivering_stamp, QueueFacts::default())
        } else {
            (
                self.paste_queue.queue.stamp_now(),
                self.paste_queue.queue.try_queue_facts().unwrap_or_default(),
            )
        };
        let facts = self.build_route_facts(queue_facts, ctx);
        let mut intent = Intent::new(class, self.shadow_origin()).stamped(stamp);
        if delivering {
            intent = intent.delivering();
        }
        let plan = table::decide(&intent, &facts, None);
        Some(ShadowPending {
            intent,
            plan,
            facts,
        })
    }

    /// Ends a legacy entry point: compares the leaf it noted with the table's plan and logs a
    /// disagreement. Nothing happens when the shadow comparison was not wanted or the chain
    /// reached no leaf this call knows (a nested entry point took it).
    pub(super) fn shadow_end(&self, pending: Option<ShadowPending>) {
        let Some(pending) = pending else {
            return;
        };
        let Some(legacy) = shadow::take_leaf() else {
            return;
        };
        let _ = shadow::check(&pending.intent, &pending.plan, &pending.facts, legacy);
    }
}

// ---- step 4: delivery through the table ---------------------------------------------------------

#[cfg(test)]
thread_local! {
    /// How many deliveries went through the legacy chain (tests only): a table-mode delivery of a
    /// replayed or ssh-held input must not add to it.
    static LEGACY_CHAIN_DELIVERIES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Records that the queue delivered an input through the legacy chain (no-op outside tests).
#[inline(always)]
pub(super) fn note_legacy_chain_delivery() {
    #[cfg(test)]
    LEGACY_CHAIN_DELIVERIES.with(|count| count.set(count.get() + 1));
}

/// The deliveries made through the legacy chain on this thread so far (tests).
#[cfg(test)]
pub(super) fn legacy_chain_deliveries() -> u64 {
    LEGACY_CHAIN_DELIVERIES.with(std::cell::Cell::get)
}

#[cfg(test)]
thread_local! {
    /// How many live inputs the table decided on this thread (tests only): an arrival the legacy
    /// chain decided does not add to it.
    static TABLE_ARRIVALS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Records that the table decided a live input (no-op outside tests).
#[inline(always)]
fn note_table_arrival() {
    #[cfg(test)]
    TABLE_ARRIVALS.with(|count| count.set(count.get() + 1));
}

/// The live inputs the table decided on this thread so far (tests).
#[cfg(test)]
pub(super) fn table_arrivals() -> u64 {
    TABLE_ARRIVALS.with(std::cell::Cell::get)
}

/// One input the queue delivers through the table: the shape it had when it was held.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Replayed<'a> {
    /// Printable characters.
    Typed(&'a str),
    /// A control key. Judged with the Enter class whatever the key is: a held key goes where the
    /// text before it went (`replayed_enter_goes_to_pty`), as legacy `replay_held_key` does.
    Key(&'a str),
    /// A control sequence (arrows, Home, End, ...).
    Seq(&'a str),
    /// Text from a paste, already escaped for the shell.
    Paste(&'a str),
    /// A signal held in the ssh segment (`HELD_SIGNAL`, OD-5), delivered to the remote side.
    Signal(SignalKind),
}

impl Replayed<'_> {
    fn class(&self) -> Class {
        match self {
            Replayed::Typed(_) => Class::Typed,
            Replayed::Key(_) => Class::Enter,
            Replayed::Seq(_) => Class::Seq,
            Replayed::Paste(_) => Class::Paste,
            Replayed::Signal(kind) => Class::Signal(*kind),
        }
    }

    fn as_held(&self) -> Option<HeldInput> {
        match self {
            Replayed::Typed(text) => Some(HeldInput::from_chars(text)),
            Replayed::Key(key) => Some(HeldInput::Key((*key).to_owned())),
            Replayed::Seq(seq) => Some(HeldInput::Seq((*seq).to_owned())),
            Replayed::Paste(text) => Some(HeldInput::Paste((*text).to_owned())),
            Replayed::Signal(_) => None,
        }
    }
}

/// The signal a `HELD_SIGNAL` entry carries (its byte as text).
fn signal_of_held_text(text: &str) -> Option<SignalKind> {
    match text {
        "\u{3}" => Some(SignalKind::Interrupt),
        "\u{4}" => Some(SignalKind::Eof),
        "\u{1a}" => Some(SignalKind::Suspend),
        "\u{1c}" => Some(SignalKind::Quit),
        _ => None,
    }
}

impl TerminalView {
    /// Delivers one ssh-held input (an `EntryKind::Held`) with the table, at the hand-over (or
    /// when a drain reaches it): the same destinations the legacy `deliver_ssh_held` reached, the
    /// signal class included.
    pub(super) fn deliver_held_via_table(
        &mut self,
        class: u8,
        text: &str,
        stamp: Stamp,
        ctx: &mut ViewContext<Self>,
    ) {
        let input = match class {
            HELD_TYPED => Replayed::Typed(text),
            HELD_KEY => Replayed::Key(text),
            HELD_PASTE => Replayed::Paste(text),
            HELD_SIGNAL => match signal_of_held_text(text) {
                Some(signal) => Replayed::Signal(signal),
                None => {
                    // Never written anywhere: a signal that is not one of the four is a bug, and
                    // a byte nobody understands must not reach the remote shell by accident.
                    log::error!("input queue: a held signal entry carried an unknown byte");
                    return;
                }
            },
            // `HeldInput::from_parts` reads every other class as a sequence.
            _ => Replayed::Seq(text),
        };
        self.deliver_via_table(input, Origin::Held, stamp, ctx);
    }

    /// The facts of this moment, after the prompt rule of the ssh hold: a host-key / password
    /// prompt appeared while keys were held, so what was typed before it may be a password and is
    /// discarded (the event-time rule, #1113); the keys after it go to the prompt. The facts are
    /// built again when the hold was discarded. A signal never discards (it is judged by its own
    /// rows).
    fn facts_after_prompt_discard(
        &mut self,
        queue: QueueFacts,
        signal: bool,
        ctx: &mut ViewContext<Self>,
    ) -> RouteFacts {
        let facts = self.build_route_facts(queue, ctx);
        // The machine says a prompt is up (a host-key / password prompt before any remote
        // session): what was held may be a password typed blind. `PromptUp` implies the gate
        // drops nothing (no pending session, no setup, a seen line of text).
        let prompt_up = matches!(facts.ssh, SshPhase::PromptUp { .. });
        if self.ssh_typeahead_holding && prompt_up && !signal {
            self.discard_held_ssh_typeahead_for_prompt(ctx);
            return self.build_route_facts(queue, ctx);
        }
        facts
    }

    /// Decides ONE delivered input with the table and executes the plan. `origin` is how the
    /// entry is judged (`Replay` for text / an Enter typed behind an Enter that waited, `Held` for
    /// an ssh-held input), `stamp` what it got at its sink. Must be called from a delivery (the
    /// queue is draining): a plan that holds the input again requeues it under the connect gate
    /// when the delivery returns.
    pub(super) fn deliver_via_table(
        &mut self,
        input: Replayed<'_>,
        origin: Origin,
        stamp: Stamp,
        ctx: &mut ViewContext<Self>,
    ) {
        let _key_path = KeyPathScope::enter(&self.key_path_depth);
        if matches!(input, Replayed::Typed(_)) {
            self.reprobe_interactive_latch_on_key();
        }
        // Step 7: no ssh refresher runs here. The gate is armed, released and handed over by the
        // events (`view/ssh_edges.rs`), and the facts carry the edge-driven machine's phase.
        // The entry was delivered by the queue, so no gate covers it: the facts carry none.
        let facts = self.facts_after_prompt_discard(
            QueueFacts::default(),
            matches!(input, Replayed::Signal(_)),
            ctx,
        );
        let intent = Intent::new(input.class(), origin)
            .stamped(stamp)
            .delivering();
        let plan = table::decide(&intent, &facts, None);
        warpui::input_order::log_key_hop("kh_table_decide", || {
            format!(
                "class={:?} origin={origin:?} row={} outcome={:?} effects={:?}",
                intent.class,
                plan.row.name(),
                plan.outcome,
                plan.effects
            )
        });
        self.execute_table_plan(input, &plan, &facts, ctx);
    }

    /// Executes `plan` for `input` with the leaf writers legacy ends in.
    pub(super) fn execute_table_plan(
        &mut self,
        input: Replayed<'_>,
        plan: &Plan,
        facts: &RouteFacts,
        ctx: &mut ViewContext<Self>,
    ) {
        if let Replayed::Signal(_) = input {
            self.execute_held_signal(input, plan, ctx);
            return;
        }
        if self.execute_shared_outcome(&input, plan, ctx) {
            return;
        }
        match input {
            Replayed::Typed(text) => self.execute_typed(text, plan, ctx),
            Replayed::Key(key) => self.execute_key(key, plan, ctx),
            Replayed::Seq(seq) => self.execute_seq(seq, plan, ctx),
            Replayed::Paste(text) => self.execute_paste(text, plan, facts, ctx),
            Replayed::Signal(_) => {}
        }
    }

    /// The outcomes every class shares: the choice card's hint, the setup dialog's swallow, a
    /// drop, and a hold behind the ssh connect. True when handled.
    fn execute_shared_outcome(
        &mut self,
        input: &Replayed<'_>,
        plan: &Plan,
        ctx: &mut ViewContext<Self>,
    ) -> bool {
        match plan.outcome {
            Outcome::Hint => {
                // #1087: printable keys while the SSH-extension choice card holds focus show its
                // "choose an option first" hint; they are not buffered.
                if let Some(choice_card) = self.ssh_choice_card_holding_keys(ctx) {
                    choice_card.update(ctx, |card, ctx| card.note_typed_before_choice(ctx));
                }
                true
            }
            Outcome::Swallow(_) => {
                // The setup dialog owns the keyboard (#917): the input is dropped and focus goes
                // back to the dialog.
                self.redetermine_global_focus(ctx);
                if !self.ssh_setup_drop_notified {
                    self.ssh_setup_drop_notified = true;
                    self.show_ssh_typing_toast(
                        "Typing is ignored while the SSH setup prompt is open",
                        ctx,
                    );
                }
                true
            }
            Outcome::Drop(why) => {
                // Counts and reasons only, never the text. A key stamped before the ssh prompt
                // (H4, OD-7) is dropped here even though it reaches the main thread after the
                // prompt was published.
                log::info!(
                    "input queue: a delivered input was dropped: row={} why={why:?}",
                    plan.row.name()
                );
                warpui::input_order::log_key_hop("kh_table_drop", || {
                    format!("row={} why={why:?}", plan.row.name())
                });
                true
            }
            Outcome::Hold(GateKind::SshConnect) => {
                // Held behind the connect again (a connect that began, or another one): the queue
                // puts it back at the front under the connect gate when this delivery returns
                // (`ssh_hold_push` requeues mid-drain, #1415).
                if let Some(held) = input.as_held() {
                    self.ssh_ensure_window_for_hold(ctx);
                    self.set_ssh_typeahead_holding(true);
                    let outcome = self.ssh_hold_push(held, ctx);
                    self.note_ssh_typeahead_outcome(&outcome, ctx);
                    self.schedule_ssh_typeahead_give_up(ctx);
                }
                true
            }
            _ => false,
        }
    }

    /// Text. Order as `typed_characters_on_terminal`: the warpification UX collapses for every
    /// key that gets past the ssh holds, then the pty (with the editor as the refused fallback),
    /// the editor, or the agent's input.
    fn execute_typed(&mut self, text: &str, plan: &Plan, ctx: &mut ViewContext<Self>) {
        self.on_ssh_warpification_key_event(Some(SshKeyEvent::from_chars(text)), ctx);
        match plan.outcome {
            Outcome::EditorInsert | Outcome::AgentInput => {
                self.insert_typed_into_editor(text, plan.effects, ctx);
            }
            Outcome::Pty(_) => {
                self.highlighted_link.invalidate();
                self.report_possible_typeahead(text);
                let written = self.write_user_bytes_to_pty_after_paste_stage(
                    Cow::Owned(text.as_bytes().to_vec()),
                    true,
                    ctx,
                );
                if written {
                    if plan.effects.contains(Effects::FOCUS_TERMINAL) {
                        self.focus_terminal(ctx);
                    }
                } else {
                    match plan.on_refused {
                        OnRefused::EditorInsert => {
                            self.insert_typed_into_editor(text, plan.effects, ctx)
                        }
                        OnRefused::Drop(why) => log::info!(
                            "input queue: the pty refused delivered text and the box is hidden: why={why:?}"
                        ),
                        OnRefused::NotApplicable => {}
                    }
                }
            }
            // A PowerShell setup holds every key: the stage keeps it in its gate.
            Outcome::Hold(GateKind::PowershellSetup) => {
                self.write_user_bytes_to_pty_after_paste_stage(
                    Cow::Owned(text.as_bytes().to_vec()),
                    true,
                    ctx,
                );
            }
            other => self.table_unexpected(Replayed::Typed(text), other, plan, ctx),
        }
    }

    /// The tail of `typed_characters_on_terminal` for text that goes to the editor.
    fn insert_typed_into_editor(
        &mut self,
        text: &str,
        effects: Effects,
        ctx: &mut ViewContext<Self>,
    ) {
        // Only clear selected blocks and text if we're not in AI mode since in AI mode we don't
        // want to clear the selected blocks or text (context) when we start typing.
        if !self.ai_render_context.borrow().is_ai_input_enabled
            && !FeatureFlag::AgentView.is_enabled()
        {
            self.clear_selected_blocks(ctx);
            self.clear_selected_text(ctx);
        }
        self.update_scroll_position_locking(ScrollPositionUpdate::AfterTypedCharacters, ctx);
        // #1028: keys typed while a card holds focus move focus to the input with them.
        if effects.contains(Effects::FOCUS_INPUT) {
            self.focus_input_box(ctx);
        }
        self.input
            .update(ctx, |input, ctx| input.system_insert(text, ctx));
        if self.focus_reclaim_active(ctx) {
            self.place_focus_after_card_resolved(ctx);
        }
    }

    /// A held control key or a replayed Enter: where the text before it went (the pty, else the
    /// editor), as `replay_held_key` / `route_after_enter_input`.
    fn execute_key(&mut self, key: &str, plan: &Plan, ctx: &mut ViewContext<Self>) {
        match plan.outcome {
            Outcome::Pty(_) | Outcome::Hold(GateKind::PowershellSetup) => {
                if matches!(plan.outcome, Outcome::Pty(_)) {
                    self.report_possible_typeahead(key);
                }
                self.write_user_bytes_to_pty_after_paste_stage(
                    Cow::Owned(key.as_bytes().to_vec()),
                    true,
                    ctx,
                );
            }
            Outcome::EditorSubmit | Outcome::EditorAction => {
                let action = match key {
                    "\r" | "\n" | "\r\n" => {
                        self.input.update(ctx, |input, ctx| input.input_enter(ctx));
                        return;
                    }
                    "\u{7f}" | "\u{8}" => EditorAction::Backspace,
                    "\u{15}" => EditorAction::DeleteAllLeft,
                    "\u{17}" => EditorAction::DeleteWordLeft,
                    _ => {
                        log::info!("input queue: a held key has no editor action and was ignored");
                        return;
                    }
                };
                self.input.update(ctx, |input, ctx| {
                    input
                        .editor()
                        .update(ctx, |editor, ctx| editor.handle_action(&action, ctx))
                });
            }
            other => self.table_unexpected(Replayed::Key(key), other, plan, ctx),
        }
    }

    /// A held escape sequence: the pty, or the editor's key after the hand-off took the line over,
    /// as `control_sequence_on_terminal`.
    fn execute_seq(&mut self, seq: &str, plan: &Plan, ctx: &mut ViewContext<Self>) {
        match plan.outcome {
            Outcome::Pty(_) => {
                self.on_ssh_warpification_key_event(
                    Some(SshKeyEvent::from_bytes(seq.as_bytes())),
                    ctx,
                );
                self.write_user_bytes_to_pty_after_paste_stage(
                    Cow::Owned(seq.as_bytes().to_vec()),
                    true,
                    ctx,
                );
            }
            Outcome::Hold(GateKind::PowershellSetup) => {
                self.write_user_bytes_to_pty_after_paste_stage(
                    Cow::Owned(seq.as_bytes().to_vec()),
                    true,
                    ctx,
                );
            }
            Outcome::EditorAction => {
                if !self.apply_key_to_editor_after_handoff(seq.as_bytes(), ctx) {
                    log::info!("input queue: a held control sequence had no editor action");
                }
            }
            other => self.table_unexpected(Replayed::Seq(seq), other, plan, ctx),
        }
    }

    /// A held paste: into the input, or to the pty (bracketed as the facts say), as
    /// `deliver_paste_text`.
    fn execute_paste(
        &mut self,
        text: &str,
        plan: &Plan,
        facts: &RouteFacts,
        ctx: &mut ViewContext<Self>,
    ) {
        match plan.outcome {
            Outcome::EditorInsert | Outcome::AgentInput => {
                self.input.update(ctx, |input, ctx| {
                    input.paste_insert(text, ctx);
                });
            }
            Outcome::PasteToPty { bracketed } => {
                self.write_user_bytes_to_pty_after_paste_stage(
                    Cow::Owned(paste_bytes_for_pty(text, bracketed)),
                    true,
                    ctx,
                );
            }
            Outcome::Hold(GateKind::PowershellSetup) => {
                self.write_user_bytes_to_pty_after_paste_stage(
                    Cow::Owned(paste_bytes_for_pty(text, facts.needs_bracketed_paste)),
                    true,
                    ctx,
                );
            }
            other => self.table_unexpected(Replayed::Paste(text), other, plan, ctx),
        }
    }

    /// A signal held in the ssh segment (`HELD_SIGNAL`, OD-5): delivered to the remote shell,
    /// after the keys typed before it (the queue's order), through the leaf so nothing holds it
    /// again. Never to the local shell: it only exists in the held segment, and the ways that
    /// segment ends without a remote shell discard it (`ssh_drop_detached`).
    fn execute_held_signal(
        &mut self,
        input: Replayed<'_>,
        plan: &Plan,
        ctx: &mut ViewContext<Self>,
    ) {
        match plan.outcome {
            Outcome::SignalToPty(signal) => {
                self.write_user_bytes_to_pty_after_paste_stage(
                    Cow::Borrowed(signal_bytes(signal)),
                    true,
                    ctx,
                );
            }
            other => {
                // Not delivered: a signal meant for the remote shell is never run anywhere else.
                log::error!(
                    "input queue: a held signal got {other:?} (row {}) and was not delivered ({input:?})",
                    plan.row.name()
                );
            }
        }
    }

    /// An outcome the delivery has no leaf for (the table, asked with no gate behind the entry,
    /// should not produce one): the input goes through the legacy chain, as it always did, and the
    /// surprise is logged. Never a drop.
    fn table_unexpected(
        &mut self,
        input: Replayed<'_>,
        outcome: Outcome,
        plan: &Plan,
        ctx: &mut ViewContext<Self>,
    ) {
        log::warn!(
            "input queue: table-mode delivery has no leaf for {outcome:?} (row {}); using the legacy chain",
            plan.row.name()
        );
        note_legacy_chain_delivery();
        // The legacy BODIES, not the entry points: those are the table's own arrival in `table`
        // mode, and a plan with no leaf must not decide the same plan again.
        match input {
            Replayed::Typed(text) => self.typed_characters_on_terminal_legacy(text, ctx),
            Replayed::Key(key) => self.replay_held_key_legacy(key, ctx),
            Replayed::Seq(seq) => self.control_sequence_on_terminal_legacy(seq.as_bytes(), ctx),
            Replayed::Paste(text) => self.deliver_paste_text(text, ctx),
            Replayed::Signal(_) => {}
        }
    }
}

// ---- step 5: arrival of terminal keys, in `table` mode ------------------------------------------
//
// In `PHOSPHOR_QUEUE_DESTINATION=table` the three legacy entry points a live key arrives at
// (`typed_characters_on_terminal`, `keydown_on_terminal`, `control_sequence_on_terminal`) hand the
// input to [`TerminalView::arrive_via_table`] instead of running their chain of predicates. It
// takes the stamp first (what the key saw when it reached the sink), runs the refreshers the legacy
// probes ran (the lazy arming of the connect gate, the subshell gates that open on the first key,
// the stale young Enter hold), builds the facts WITH the queue's own (open gates, queued entries,
// the mailbox), decides once and executes the plan with the same leaf writers legacy ended in. The
// idle path (nothing held, nothing queued) is the plan `Pty` executed at once; a key a gate holds
// is offered to the queue as destination-neutral content (`Text`, `Enter`, or a `Held` key), which
// the queue's delivery decides again, with the facts of THAT moment, when the gate opens
// (`deliver_via_table`). The deliberate changes of the owner's decisions are in the table: ^Z and
// ^\ jump the queue like ^C (OD-1), a signal typed in the ssh setup window is held and delivered to
// the remote shell (OD-5), a live Enter while the agent owns the pty is dropped (OD-2).
//
// Not routed through neutral entries: the keys that wait behind the typeahead hand-off's gates
// (`LineEditorActive`, `PromptSeen`, `HandoffReport`) and the PowerShell setup. Those gates give
// their keys back as BYTES (`bytes_of_taken_entries`, the hand-off's own take-and-write), so the
// key is written through the legacy writer, which queues it as bytes. Step 8 retires that.

thread_local! {
    /// The origin an input arriving at a legacy entry point is judged with: a keyboard key unless
    /// the writer of another origin says so (synced input, the paced CLI-agent submit, a shared
    /// session viewer) around its call. Thread-local like the shadow's leaf: the entry points are
    /// plain methods with no room for another argument.
    static ARRIVAL_ORIGIN: Cell<Origin> = const { Cell::new(Origin::Keyboard) };
}

/// Restores the previous arrival origin when dropped.
#[must_use = "the origin is restored when the guard drops"]
pub(super) struct ArrivalOriginGuard(Origin);

impl Drop for ArrivalOriginGuard {
    fn drop(&mut self) {
        ARRIVAL_ORIGIN.with(|slot| slot.set(self.0));
    }
}

/// Judges the inputs that arrive until the guard drops with `origin`.
pub(super) fn set_arrival_origin(origin: Origin) -> ArrivalOriginGuard {
    ArrivalOriginGuard(ARRIVAL_ORIGIN.with(|slot| slot.replace(origin)))
}

fn arrival_origin() -> Origin {
    ARRIVAL_ORIGIN.with(Cell::get)
}

/// A live input arriving at one of the three legacy entry points.
#[derive(Clone, Copy, Debug)]
pub(super) enum Arrival<'a> {
    /// `typed_characters_on_terminal`: printable characters (a signal byte typed as text is a
    /// signal).
    Typed(&'a str),
    /// `keydown_on_terminal`: Enter, a control key or a signal.
    Key(&'a str),
    /// `control_sequence_on_terminal`: arrows, Home, End, Tab, ... (and ^C / ^D).
    Seq(&'a str),
}

impl<'a> Arrival<'a> {
    fn text(self) -> &'a str {
        match self {
            Arrival::Typed(text) | Arrival::Key(text) | Arrival::Seq(text) => text,
        }
    }

    fn class(self) -> Class {
        if let Some(signal) = signal_of_held_text(self.text()) {
            return Class::Signal(signal);
        }
        match self {
            Arrival::Typed(_) => Class::Typed,
            Arrival::Key("\r") | Arrival::Key("\n") => Class::Enter,
            Arrival::Key(_) => Class::Control,
            Arrival::Seq(_) => Class::Seq,
        }
    }

    /// The same input in the form the delivery's executors take.
    fn as_replayed(self) -> Replayed<'a> {
        match self {
            Arrival::Typed(text) => Replayed::Typed(text),
            Arrival::Key(key) => Replayed::Key(key),
            Arrival::Seq(seq) => Replayed::Seq(seq),
        }
    }
}

/// Logs (counts and the row only, never content) an input the table dropped.
fn log_table_drop(plan: &Plan, why: DropWhy) {
    log::info!(
        "input queue: an input was dropped: row={} why={why:?}",
        plan.row.name()
    );
    warpui::input_order::log_key_hop("kh_table_drop", || {
        format!("row={} why={why:?}", plan.row.name())
    });
}

impl TerminalView {
    /// Whether a live input arriving at a legacy entry point is decided by the table now: the mode
    /// is `table` and the queue is not delivering (a delivery is decided by `deliver_via_table`).
    pub(super) fn arrives_via_table(&self) -> bool {
        destination_mode() == DestinationMode::Table && !self.paste_queue.delivering
    }

    /// Decides a live input with the table and executes the plan. See the section comment.
    pub(super) fn arrive_via_table(&mut self, arrival: Arrival<'_>, ctx: &mut ViewContext<Self>) {
        note_table_arrival();
        // First: what the key saw when it reached the sink (H4 judges a key by it).
        let stamp = self.paste_queue.queue.stamp_now();
        let _key_path = KeyPathScope::enter(&self.key_path_depth);
        let class = arrival.class();
        let text = arrival.text();
        let origin = arrival_origin();
        let is_signal = matches!(class, Class::Signal(_));
        if !matches!(arrival, Arrival::Seq(_)) {
            self.reprobe_interactive_latch_on_key();
        }
        // Operations that found the queue busy come before this key (typed order).
        self.flush_deferred_queue_ops(ctx);
        // Step 7: no ssh refresher runs on the key path (the connect gate is armed, released and
        // handed over by the events, `view/ssh_edges.rs`); the facts carry the machine's phase.
        if !is_signal {
            // A young Enter's hold the hand-off's state has outgrown ends before this key is
            // judged, and the subshell gates the first key opens are open when the facts are read.
            self.settle_stale_young_hold_before_key(ctx);
            let _ = self.prepare_subshell_gates(text.as_bytes(), ctx);
        }
        let queue_facts = self.paste_queue.queue.try_queue_facts().unwrap_or_default();
        let facts = self.facts_after_prompt_discard(queue_facts, is_signal, ctx);
        let intent = Intent::new(class, origin).stamped(stamp);
        let plan = table::decide(&intent, &facts, None);
        warpui::input_order::log_key_hop("kh_table_decide", || {
            format!(
                "class={class:?} origin={origin:?} row={} outcome={:?} effects={:?} arrival=1",
                plan.row.name(),
                plan.outcome,
                plan.effects
            )
        });
        self.execute_arrival(arrival, &plan, stamp, origin, ctx);
    }

    fn execute_arrival(
        &mut self,
        arrival: Arrival<'_>,
        plan: &Plan,
        stamp: Stamp,
        origin: Origin,
        ctx: &mut ViewContext<Self>,
    ) {
        if let Class::Signal(signal) = arrival.class() {
            self.execute_arrival_signal(arrival.text(), signal, plan, ctx);
            return;
        }
        match plan.outcome {
            // A key nothing takes (D10): the selection and the focus, as the keydown path always
            // did.
            Outcome::Drop(why) if plan.effects.contains(Effects::REFOCUS_GLOBAL) => {
                log_table_drop(plan, why);
                self.refocus_after_dropped_key(ctx);
                return;
            }
            // Behind a gate of the queue (H5, H6, H7, H8) or behind queued entries (H8-queue).
            Outcome::Hold(gate) if gate != GateKind::SshConnect => {
                self.hold_arrival(arrival, gate, stamp, origin, ctx);
                return;
            }
            Outcome::Queue => {
                self.offer_arrival_to_queue(arrival, stamp, origin, ctx);
                return;
            }
            _ => {}
        }
        let input = arrival.as_replayed();
        if self.execute_shared_outcome(&input, plan, ctx) {
            return;
        }
        match arrival {
            Arrival::Typed(text) => self.execute_typed(text, plan, ctx),
            Arrival::Key(key) => self.execute_key_arrival(key, plan, ctx),
            Arrival::Seq(seq) => self.execute_seq(seq, plan, ctx),
        }
    }

    /// The tail `keydown_on_terminal` ran for a key nothing took: clear the selection, scroll,
    /// and put focus where it belongs.
    fn refocus_after_dropped_key(&mut self, ctx: &mut ViewContext<Self>) {
        self.clear_selected_blocks(ctx);
        self.clear_selected_text(ctx);
        self.update_scroll_position_locking(ScrollPositionUpdate::AfterKeydownOnTerminal, ctx);
        self.redetermine_global_focus(ctx);
    }

    /// The ssh warpification UX collapses for every key that gets past the ssh holds.
    fn note_arrival_for_warpification(
        &mut self,
        arrival: Arrival<'_>,
        ctx: &mut ViewContext<Self>,
    ) {
        let event = match arrival {
            Arrival::Typed(text) | Arrival::Key(text) => SshKeyEvent::from_chars(text),
            Arrival::Seq(seq) => SshKeyEvent::from_bytes(seq.as_bytes()),
        };
        self.on_ssh_warpification_key_event(Some(event), ctx);
    }

    /// A key the table holds behind `gate` (H5-H8).
    fn hold_arrival(
        &mut self,
        arrival: Arrival<'_>,
        gate: GateKind,
        stamp: Stamp,
        origin: Origin,
        ctx: &mut ViewContext<Self>,
    ) {
        match gate {
            // The Enter that waits for its verdict (or for the route it submitted to): what is
            // typed behind it is the input's own queue entry, released in order once it has run.
            GateKind::EnterVerdict | GateKind::TabCompletion => {
                let after = match arrival {
                    Arrival::Typed(text) => Some(AfterEnterInput::Text(text.to_owned())),
                    Arrival::Key("\r") | Arrival::Key("\n") => Some(AfterEnterInput::Enter),
                    _ => None,
                };
                if let Some(after) = after {
                    let queued = self
                        .input
                        .update(ctx, |input, ctx| input.queue_behind_enter(after, ctx));
                    if queued {
                        return;
                    }
                }
                self.offer_arrival_to_queue(arrival, stamp, origin, ctx);
            }
            // The hand-off's gates and the PowerShell setup give their keys back as bytes: the key
            // is written through the legacy writer, which queues it as bytes behind them.
            GateKind::LineEditorActive
            | GateKind::PromptSeen
            | GateKind::HandoffReport
            | GateKind::PowershellSetup => self.hold_arrival_as_bytes(arrival, ctx),
            _ => self.offer_arrival_to_queue(arrival, stamp, origin, ctx),
        }
    }

    /// The key goes through the legacy writer (`write_user_bytes_to_pty`), which holds it behind
    /// the gate the table named.
    fn hold_arrival_as_bytes(&mut self, arrival: Arrival<'_>, ctx: &mut ViewContext<Self>) {
        self.note_arrival_for_warpification(arrival, ctx);
        self.highlighted_link.invalidate();
        if !matches!(arrival, Arrival::Seq(_)) {
            self.report_possible_typeahead(arrival.text());
        }
        self.write_user_bytes_to_pty(arrival.text().as_bytes().to_vec(), ctx);
    }

    /// Offers the key to the pane queue as destination-neutral content, with the stamp and the
    /// origin it arrived with: where it goes is decided when the queue delivers it.
    fn offer_arrival_to_queue(
        &mut self,
        arrival: Arrival<'_>,
        stamp: Stamp,
        origin: Origin,
        ctx: &mut ViewContext<Self>,
    ) {
        self.note_arrival_for_warpification(arrival, ctx);
        let kind = match arrival {
            Arrival::Typed(text) => EntryKind::Text(text.to_owned()),
            Arrival::Key("\r") | Arrival::Key("\n") => EntryKind::Enter,
            Arrival::Key(key) => EntryKind::Held {
                class: HELD_KEY,
                text: key.to_owned(),
            },
            Arrival::Seq(seq) => EntryKind::Held {
                class: HELD_SEQ,
                text: seq.to_owned(),
            },
        };
        let queue = self.paste_queue.queue.clone();
        let Some(mut guard) = queue.try_lock() else {
            // Busy: the key waits its turn, in order, with the stamp it has now.
            queue.defer(Deferred::Offer {
                kind,
                origin,
                stamp,
            });
            return;
        };
        let mut host = PtyQueueHost::new(self, ctx);
        guard.offer_stamped(&mut host, kind, origin, stamp);
    }

    /// A control key (Enter included) the table sends on: to the pty, or to the editor that took
    /// input over from it (D10).
    fn execute_key_arrival(&mut self, key: &str, plan: &Plan, ctx: &mut ViewContext<Self>) {
        match plan.outcome {
            Outcome::Pty(_) => {
                self.on_ssh_warpification_key_event(Some(SshKeyEvent::from_chars(key)), ctx);
                self.highlighted_link.invalidate();
                self.report_possible_typeahead(key);
                self.write_user_bytes_to_pty_after_paste_stage(
                    Cow::Owned(key.as_bytes().to_vec()),
                    true,
                    ctx,
                );
            }
            Outcome::EditorAction | Outcome::EditorSubmit => {
                if !self.apply_key_to_editor_after_handoff(key.as_bytes(), ctx) {
                    self.refocus_after_dropped_key(ctx);
                }
            }
            other => {
                log::warn!(
                    "input queue: table-mode arrival has no leaf for {other:?} (row {}); using the legacy chain",
                    plan.row.name()
                );
                note_legacy_chain_delivery();
                self.keydown_on_terminal_legacy(key, ctx);
            }
        }
    }

    /// A signal (^C ^D ^Z ^\) typed or pressed: held in the ssh setup window and delivered to the
    /// remote shell (P1a, OD-5), or sent on through the legacy writer, which applies the queue's
    /// per-gate rule (`signal_plan`) and jumps the keys held in front of it.
    fn execute_arrival_signal(
        &mut self,
        text: &str,
        signal: SignalKind,
        plan: &Plan,
        ctx: &mut ViewContext<Self>,
    ) {
        if plan.outcome == Outcome::HoldSignal {
            // In typed order with the keys held there; delivered at the hand-over, never to the
            // local shell.
            self.note_ssh_interrupt_held(ctx);
            self.ssh_ensure_window_for_hold(ctx);
            self.set_ssh_typeahead_holding(true);
            let pushed = self.ssh_hold_push_signal(signal, ctx);
            self.note_ssh_typeahead_outcome(&pushed, ctx);
            self.schedule_ssh_typeahead_give_up(ctx);
            return;
        }
        // The interrupt passes, so what was typed before it is stale, as the tty's own input
        // flush would make it (P1b).
        if plan.effects.contains(Effects::RESET_SSH_HOLD) {
            self.reset_ssh_typeahead();
        }
        // An Enter that waits for its verdict is void.
        if self.input.as_ref(ctx).enter_queue_holds_keys(ctx) {
            self.input
                .update(ctx, |input, ctx| input.abort_enter_queue(ctx));
        }
        match plan.outcome {
            Outcome::PowershellAbort => self.ctrl_c_during_powershell_setup(ctx),
            Outcome::SigintForeground => {
                self.on_ssh_warpification_key_event(Some(SshKeyEvent::from_chars(text)), ctx);
                self.highlighted_link.invalidate();
                // #1178/#1253: the SIGINT path marks the ssh command interrupted too (left in the
                // model's inbox, which applies it before it parses the next bytes).
                self.leave_user_wrote_in_model_inbox(false, true);
                ctx.emit(Event::SigintForegroundToPty);
            }
            Outcome::AbortBytes => {
                self.on_ssh_warpification_key_event(Some(SshKeyEvent::from_chars(text)), ctx);
                self.highlighted_link.invalidate();
                // #1001: a Ctrl-C between the SSH hook and InitShell aborts the connect (`~.`).
                let bytes = self
                    .ssh_pre_init_abort_bytes()
                    .unwrap_or_else(|| signal_bytes(signal).to_vec());
                self.write_user_bytes_to_pty(bytes, ctx);
            }
            // The writer applies the rest: the stash during the subshell bootstrap lines (P1d),
            // the queue's rule for each open gate (P2), the oracle and the stage (D6).
            Outcome::SignalToPty(_) | Outcome::SignalAtGates | Outcome::Stash => {
                self.on_ssh_warpification_key_event(Some(SshKeyEvent::from_chars(text)), ctx);
                self.highlighted_link.invalidate();
                self.report_possible_typeahead(text);
                self.write_user_bytes_to_pty(signal_bytes(signal).to_vec(), ctx);
            }
            Outcome::Drop(why) => {
                log_table_drop(plan, why);
                if plan.effects.contains(Effects::REFOCUS_GLOBAL) {
                    self.refocus_after_dropped_key(ctx);
                }
            }
            other => {
                // Not delivered: a signal is never run anywhere the table did not name.
                log::error!(
                    "input queue: a signal got {other:?} (row {}) and was not delivered",
                    plan.row.name()
                );
            }
        }
    }
}

// ---- step 5/6: the rows of the writers of another origin ----------------------------------------

/// What a writer of another origin (synced input, the paced CLI-agent submit, a shared-session
/// viewer) learned from the table before it wrote.
pub(super) struct OriginRow {
    shadow: Option<ShadowPending>,
    refused: bool,
}

impl OriginRow {
    /// The table drops the input at its origin row (`O-synced`): the writer writes nothing.
    pub(super) fn refused(&self) -> bool {
        self.refused
    }
}

impl TerminalView {
    /// Starts a writer of `origin`. In `table` mode the table decides the input (`class`, `origin`)
    /// and a drop at the origin row refuses the write (the other rows are what the writers' stages
    /// apply themselves; the plan is logged); in `shadow` mode the table is compared with what the
    /// writer did, once it ends ([`Self::end_origin_row`]).
    pub(super) fn begin_origin_row(
        &mut self,
        class: Class,
        origin: Origin,
        ctx: &mut ViewContext<Self>,
    ) -> OriginRow {
        let _origin = set_arrival_origin(origin);
        match destination_mode() {
            DestinationMode::Table => {
                let stamp = self.paste_queue.queue.stamp_now();
                // The connect gate is armed lazily from the warpify state, as the legacy probes
                // (`agent_pty_write_gate`, the hold) did before they read it.
                let _ = self.refresh_ssh_connect_gate();
                let queue_facts = self.paste_queue.queue.try_queue_facts().unwrap_or_default();
                let facts = self.build_route_facts(queue_facts, ctx);
                let intent = Intent::new(class, origin).stamped(stamp);
                let plan = table::decide(&intent, &facts, None);
                warpui::input_order::log_key_hop("kh_table_decide", || {
                    format!(
                        "class={class:?} origin={origin:?} row={} outcome={:?} effects={:?} writer=1",
                        plan.row.name(),
                        plan.outcome,
                        plan.effects
                    )
                });
                if let Outcome::Drop(why) = plan.outcome {
                    log_table_drop(&plan, why);
                }
                OriginRow {
                    shadow: None,
                    // The rows that refuse a write at its origin: the synced origin (no running
                    // command) and the agent's write (a setup owns the pty).
                    refused: matches!(plan.outcome, Outcome::Drop(_))
                        && matches!(plan.row, Row::OSynced | Row::D13),
                }
            }
            _ => OriginRow {
                shadow: self.shadow_begin(class, ctx),
                refused: false,
            },
        }
    }

    /// Ends a writer of another origin: `written` is whether the bytes reached the pty (or the
    /// queue that holds them for it). The shadow comparison reports a difference.
    pub(super) fn end_origin_row(&self, row: OriginRow, written: bool) {
        if row.shadow.is_some() {
            shadow::note_leaf(if written {
                DestClass::Pty
            } else {
                DestClass::Dropped
            });
        }
        self.shadow_end(row.shadow);
    }

    /// A navigation key from the key bindings (an arrow, Page Up / Down, Home, End) goes to the pty
    /// when `legacy` says so, else it scrolls or moves the block selection. Not routed by the table:
    /// the fall-back is a scroll, which no table row models, and the predicates differ by design
    /// (B11: `terminal_up` ignores the agent, `page_up` ignores the hand-off); this only COMPARES
    /// them in `shadow` mode, with the table's plan for a control sequence, and returns `legacy`.
    pub(super) fn nav_key_goes_to_pty(
        &mut self,
        legacy: bool,
        ctx: &mut ViewContext<Self>,
    ) -> bool {
        let pending = self.shadow_begin(Class::Seq, ctx);
        if pending.is_some() {
            shadow::note_leaf(if legacy {
                DestClass::Pty
            } else {
                DestClass::Dropped
            });
        }
        self.shadow_end(pending);
        legacy
    }

    /// `paste_routing` decided by the table (step 6, `table` mode): whether a paste made now goes
    /// into the input (else to the pty), and whether the pty paste is bracketed. The destination
    /// rows only (D1, D2, D5-D7): the paste read takes its own place in the queue (a
    /// `PasteReading` gate), so no queue fact is passed, and nothing blocks on the model's lock.
    pub(super) fn paste_routing_via_table(&mut self, ctx: &mut ViewContext<Self>) -> (bool, bool) {
        let facts = self.build_route_facts(QueueFacts::default(), ctx);
        // The focus of THIS moment (a paste is not a key of the pane sink, so the focus the last
        // key noted may be stale).
        let mut stamp = self.paste_queue.queue.stamp_now();
        stamp.focus = facts.focus;
        let intent = Intent::new(Class::Paste, arrival_origin())
            .stamped(stamp)
            .delivering();
        let plan = table::decide(&intent, &facts, None);
        warpui::input_order::log_key_hop("kh_table_decide", || {
            format!(
                "class=Paste row={} outcome={:?} effects={:?} routing=1",
                plan.row.name(),
                plan.outcome,
                plan.effects
            )
        });
        let into_input = matches!(plan.outcome, Outcome::EditorInsert | Outcome::AgentInput);
        (into_input, facts.needs_bracketed_paste)
    }
}

// ---- step 7: focus changes and the line pin ---------------------------------------------------
//
// Both are `table`-mode behaviour (the default modes keep the queue exactly as it was): the
// legacy chain's destination for queued text uses the pin, and the table's decision ignores it.

impl TerminalView {
    /// Focus moved (the terminal view gained or lost it, or the input editor did): the queue's
    /// focus epoch advances and a head that waited on destination facts is drained. A focus flip
    /// does NOT clear the line pin (it never splits a line). When the queue is busy the change is
    /// recorded as a deferred operation, applied in order.
    pub(super) fn note_queue_focus_changed(&mut self, ctx: &mut ViewContext<Self>) {
        if destination_mode() != DestinationMode::Table {
            return;
        }
        let queue = self.paste_queue.queue.clone();
        let Some(mut guard) = queue.try_lock() else {
            queue.defer(Deferred::FocusChanged);
            return;
        };
        let mut host = PtyQueueHost::new(self, ctx);
        guard.on_focus_changed(&mut host);
    }

    /// An owner-change edge (the ssh phase moved, the ssh hand-back, the hand-off's owner changed,
    /// the block changed): the line pinned to a destination is over. The queue clears the pin at
    /// its next lock, so an edge in the middle of a delivery records no mailbox operation.
    pub(super) fn request_queue_pin_clear(&self) {
        if destination_mode() == DestinationMode::Table {
            self.paste_queue.queue.route_state().request_pin_clear();
        }
    }
}
