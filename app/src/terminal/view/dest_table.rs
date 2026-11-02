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
//! `delivering` / `requeue_request` plumbing). [`TerminalView::deliver_via_table`] runs the
//! refreshers the legacy ssh probes ran (the lazy arming of the connect gate, the prompt
//! discard), builds the facts of THIS moment, decides ONCE with `table::decide` (the entry's own
//! stamp, no gate behind it: the queue delivered it, so none covers it) and executes the plan with
//! the same leaf writers legacy ends in. A plan that holds the input behind the ssh connect
//! requeues it exactly as `ssh_hold_push` always did mid-drain. Everything else (arrival of live
//! keys, the editor, paste reads, the idle path) is still legacy; those are steps 5-8.
//!
//! Never logged: the text of an input (it can be a password). Rows, classes and counts only.

use std::borrow::Cow;

use instant::Instant;
use warpui::ViewContext;

use super::paste_queue::signal_bytes;
use super::ssh_typeahead::HeldInput;
use super::{
    KeyPathScope, SshKeyEvent, TerminalView, paste_bytes_for_pty, ssh_bootstrap_swallow_active,
};
use crate::editor::EditorAction;
use crate::features::FeatureFlag;
use crate::terminal::block_list_viewport::ScrollPositionUpdate;
use crate::terminal::input_queue::shadow;
use crate::terminal::input_queue::table::{self, Class, Effects, Intent, OnRefused, Outcome, Plan};
use crate::terminal::input_queue::{
    GateKind, HELD_KEY, HELD_PASTE, HELD_SIGNAL, HELD_TYPED, Origin, QueueFacts, RouteFacts,
    SignalKind, Stamp,
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
            Origin::Keyboard
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

    /// The legacy ssh probes' refreshers, in their order (`hold_input_during_ssh_connect`): the
    /// lazy arming of the connect gate for an ssh recognised at preexec only, the hand-over of
    /// what a gate that released since the last event still holds, the bootstrap swallow clock
    /// and the sticky "visible text seen" bit. They mutate view state and stay on the delivery
    /// path exactly where they were (design OD-6: they move off it in step 7). The facts the
    /// table decides on are built AFTER them.
    fn refresh_ssh_state_for_table(&mut self, ctx: &mut ViewContext<Self>) {
        let gate_armed = self.refresh_ssh_connect_gate();
        if !gate_armed && self.ssh_typeahead_holding {
            self.deliver_ssh_typeahead_if_ready("key", ctx);
        }
        let pending_sid = if gate_armed || self.has_pending_ssh_command() {
            self.model_facts().pending_session_id
        } else {
            None
        };
        let _ = ssh_bootstrap_swallow_active(
            pending_sid,
            self.ssh_bootstrap_failed_session,
            &mut self.ssh_bootstrap_swallow_started,
            Instant::now(),
        );
        let _ = self.ssh_connect_gate_drops_keys();
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
        // No signal reaches the ssh refreshers: it is delivered where the remote shell is.
        if !matches!(input, Replayed::Signal(_)) {
            self.refresh_ssh_state_for_table(ctx);
        }
        // The entry was delivered by the queue, so no gate covers it: the facts carry none.
        let mut facts = self.build_route_facts(QueueFacts::default(), ctx);
        // A host-key / password prompt appeared while keys were held: what was typed before it
        // existed may be a password, so it is discarded (the event-time rule, #1113); the keys
        // after it go to the prompt.
        let gate_drops = facts.ssh_dialog_up
            || facts.ssh_setup_in_flight
            || facts.ssh_bootstrapping
            || facts.ssh_connecting_silently;
        if self.ssh_typeahead_holding
            && !gate_drops
            && !matches!(input, Replayed::Signal(_))
            && self.discard_ssh_typeahead_on_prompt(ctx)
        {
            facts = self.build_route_facts(QueueFacts::default(), ctx);
        }
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
        match input {
            Replayed::Typed(text) => self.typed_characters_on_terminal(text, ctx),
            Replayed::Key(key) => self.replay_held_key(key, ctx),
            Replayed::Seq(seq) => self.control_sequence_on_terminal(seq.as_bytes(), ctx),
            Replayed::Paste(text) => self.deliver_paste_text(text, ctx),
            Replayed::Signal(_) => {}
        }
    }
}
