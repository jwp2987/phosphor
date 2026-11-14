//! The terminal view's side of the destination table (#1424): the ONLY destination path since
//! #1353 step 8 (the legacy chain of predicates, its `PHOSPHOR_QUEUE_DESTINATION=legacy` kill
//! switch and the `shadow` comparison are gone; `docs/design/r17-step8.md`).
//!
//! **Delivery through the table (step 4, the #1415 fix).** The queue's delivery of text and Enters
//! typed behind an Enter that waited, and of every input the ssh connect typeahead held
//! (`EntryKind::Held`), is decided here. [`TerminalView::deliver_via_table`] builds the facts of
//! THIS moment (the edge-driven `SshPhase` of `view/ssh_edges.rs` carries the connect state;
//! nothing about the connect is read from the screen, #1611, #1612, #1621), decides ONCE with
//! `table::decide` (the entry's own stamp, no gate behind it: the queue delivered it, so none
//! covers it) and executes the plan with the leaf writers. A plan that holds the input behind the
//! ssh connect requeues it exactly as `ssh_hold_push` always did mid-drain.
//!
//! **Arrival of terminal keys (step 5)** (the section below `Arrival`): a live key is decided by
//! the table at the three entry points, the idle path included; the origin rows of the writers of
//! other origins (synced input, the paced CLI-agent submit, the viewer) and the H8 editor column
//! are wired.
//!
//! **Paste, the agent's writes and the audit of the pty write call sites (step 6, R11).** A
//! terminal paste is routed by the destination rows (`paste_plan_via_table`); the agent's write
//! consults row D13 at its writer. Every site that puts user-originated bytes on the pty, and what
//! became of it:
//!
//! | site | status | why |
//! |---|---|---|
//! | `typed_characters_on_terminal`, `keydown_on_terminal`, `control_sequence_on_terminal` | ROUTED (`arrive_via_table`; bytes that are not text: `arrive_sequence_bytes_via_table`) | live keys |
//! | the replayed text / Enter (`deliver_replayed_entry_table`), the ssh-held inputs (`deliver_held_via_table`) | ROUTED (step 4, `deliver_via_table`) | deliveries |
//! | `deliver_paste_text` (a routed paste read for an ssh hold / a CLI agent) | NOT ROUTED | routed as a paste made now by `paste_routing`, the predicate rows D5-D7 state |
//! | `paste` (the routing decision) | ROUTED (`paste_plan_via_table`) | a paste read takes its own `PasteReading` gate; the table decides the destination only |
//! | `write_to_pty_for_syncing_long_running_commands` | ROUTED (origin row `O-synced`, judged as synced input) | text goes through the arrival, raw bytes through `begin_origin_row` |
//! | `write_cli_submit_bytes` (9 callers in `use_agent_footer`) | WIRED (origin row `O-cli-submit`, #1601b: modelled as the legacy writer treats the bytes -- PACED (a `CliSubmit` gate is open) held only by an open PowerShell setup (`H6`), UNPACED also held by the Enter-verdict / tab-completion gates (`H5`), the subshell gates (`H7`), any other gate but an ssh-only set (`H8`) and queued entries (`H8-queue`); both consult the hand-off (`CONSULT_ORACLE`). The plan is logged; the bytes always go through the writer) | the paced submit owns a `CliSubmit` gate and writes into the stage; the unpaced one is offered to the queue |
//! | `write_viewer_bytes_to_pty` | WIRED (origin `Viewer`; test-only writer in this tree) | no production caller |
//! | `write_agent_bytes_to_pty` | ROUTED (row `D13`, refusal) | refused while a setup owns the pty, as `agent_pty_write_gate` |
//! | `terminal_up` / `terminal_down` / `page_up` / `page_down` / `move_home` / `move_end` | NOT ROUTED | their fall-back is a scroll or a block-selection move, which no row models, and their predicates differ by design (B11); `terminal_up` writes passively |
//! | `alt_scroll`, `alt_mouse_action` (`write_passive_input_to_pty`) | NOT ROUTED | pointer-driven at wheel / mouse rate: building facts per event breaks the 2 ms budget; the only rule (`D3`, drop when the agent is in control) is the stage's own |
//! | `user_write_ctrl_c_to_pty`, `ctrl_d`, `InputEvent::CtrlD` (and `Input::ctrl_d`'s `Event::CtrlD`) | NOT ROUTED | explicit Stop / Escape / Ctrl-C / Ctrl-D actions with their own marks (`mark_interrupted_by_ctrl_c`, `ignore_bootstrapping_messages`); they write through the queue's writer and ask the same ssh facts (`ssh_connect_holds_eot`); the signal rows are the keyboard's |
//! | `try_send_text_to_cli_agent_or_rich_input`, `use_agent_footer` (2 text writes) | NOT ROUTED | CLI-agent plumbing that always means the pty; through the queue's writer (#1433) |
//! | `deliver_powershell_held_bytes`, `release_held_keys`, `apply_young_hold_end`, `release_young_hold_keys`, `release_queued_prompt_hold`, `write_given_up_chunks_to_pty` (a give-up's keys, owner decision 2026-11-09) | NOT ROUTED | take-and-write: the hand-off's and the PowerShell gate's keys come back as BYTES, which the design keeps (R3; #1353 step 8 kept them too: retiring them changes how held keys are released, `docs/design/r17-step8.md`) |
//! | `write_init_subshell_bytes_to_pty`, `write_subshell_bootstrap_line`, `clear_line_editor_and_write_to_pty_with_mac_workaround_hack` (the ssh warpify payloads), `ctrl_c_during_powershell_setup` (raw ETX) | NOT ROUTED (allowlisted doors) | app-generated setup bytes that OWN the setup gates, or must reach the console ahead of the PowerShell gate's keys |
//! | `maybe_report_focus_in/out`, the OSC 52 reply | NOT ROUTED (allowlisted doors) | terminal protocol answers, never user text |
//! | the editor's Enter submit (`PtyIntent::ExecuteCommand`) and the nested write of a delivery (#1418) | NOT ROUTED | the nested write needs follow-ups placed at the delivered entry's position (`EntryId` as a `(major, minor)` pair, R11), which touches every barrier comparison: not done (`docs/design/r17-step8.md`) |
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
use crate::terminal::input_queue::table::{
    self, Class, DropWhy, Effects, Intent, OnRefused, Outcome, Plan, Row,
};
use crate::terminal::input_queue::{
    Deferred, EntryKind, GateKind, HELD_KEY, HELD_PASTE, HELD_SEQ, HELD_SIGNAL, HELD_TYPED, Origin,
    QueueFacts, RouteFacts, SignalKind, Stamp,
};

// ---- delivery through the table (step 4) ----------------------------------------------------

#[cfg(test)]
thread_local! {
    /// How many live inputs the table decided on this thread (tests only).
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

#[cfg(test)]
thread_local! {
    /// How many plans reached an executor with no leaf for them on this thread (tests only). It
    /// must stay at zero: `table_no_leaf` and the arrival's last arm are not reachable.
    static NO_LEAF_HITS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Records a plan no executor had a leaf for (no-op outside tests; the caller logs it).
#[inline(always)]
fn note_no_leaf() {
    #[cfg(test)]
    NO_LEAF_HITS.with(|count| count.set(count.get() + 1));
}

/// The plans that found no leaf on this thread so far (tests).
#[cfg(test)]
pub(super) fn no_leaf_hits() -> u64 {
    NO_LEAF_HITS.with(std::cell::Cell::get)
}

/// One input the queue delivers through the table: the shape it had when it was held.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Replayed<'a> {
    /// Printable characters.
    Typed(&'a str),
    /// A control key. Judged with the Enter class whatever the key is: a held key goes where the
    /// text before it went (`table::pty_enter_replay`).
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

    /// The text this input writes, none for a signal.
    fn text(&self) -> Option<&str> {
        match self {
            Replayed::Typed(t) | Replayed::Key(t) | Replayed::Seq(t) | Replayed::Paste(t) => {
                Some(t)
            }
            Replayed::Signal(_) => None,
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
    /// when a drain reaches it), the signal class included.
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
        let held_typed = class == HELD_TYPED;
        self.paste_queue.delivering_held_typed = held_typed;
        self.paste_queue.delivering_ssh_held = true;
        self.deliver_via_table(input, Origin::Held, stamp, ctx);
        self.paste_queue.delivering_ssh_held = false;
        if held_typed {
            self.paste_queue.delivering_held_typed = false;
        }
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
        let (plan, facts) = self.decide_delivery(input, origin, stamp, ctx);
        self.execute_table_plan(input, &plan, &facts, ctx);
    }

    /// The decision half of [`Self::deliver_via_table`]: the plan and facts the table gives
    /// `input` delivered by the queue. The caller holds the key-path scope and executes the plan
    /// (#1585: an editor paste is decided here and keeps its own delivery when the table says
    /// editor).
    pub(super) fn decide_delivery(
        &mut self,
        input: Replayed<'_>,
        origin: Origin,
        stamp: Stamp,
        ctx: &mut ViewContext<Self>,
    ) -> (Plan, RouteFacts) {
        if matches!(input, Replayed::Typed(_)) {
            self.reprobe_interactive_latch_on_key();
        }
        // Step 7: no ssh refresher runs here. The gate is armed, released and handed over by the
        // events (`view/ssh_edges.rs`), and the facts carry the edge-driven machine's phase. No
        // prompt rule either: a held input reaching the pty is ssh's, whatever the screen shows
        // (#1611, #1612, #1621). The entry was delivered by the queue, so no gate covers it: the
        // facts carry none.
        // #1854 (R8): the secret-read check and the subshell gates run for the delivered key as
        // they do for a live one (the writer runs them in `write_user_bytes_to_pty_inner` per
        // chunk): a password must reach only the program reading it, and the setup must not be
        // typed into a secret prompt. A signal is not such a key.
        if let Some(text) = input.text() {
            self.secret_read_on_key_path_cancels_written_setup(ctx);
            let _ = self.prepare_subshell_gates(text.as_bytes(), ctx);
        }
        let mut facts = self.build_route_facts(QueueFacts::default(), ctx);
        if table::delivery_reads_setup_as_announced(&mut facts) {
            // Counts only, never the text.
            warpui::input_order::log_key_hop("kh_table_delivery_past_setup", || {
                format!("class={:?} origin={origin:?}", input.class())
            });
        }
        let mut intent = Intent::new(input.class(), origin)
            .stamped(stamp)
            .delivering();
        // #1893/#1862: the delivered entry's own synced-ness (the queue entry's origin), since a
        // delivery is judged as Held/Replay.
        if matches!(origin, Origin::Synced) || self.paste_queue.delivering_synced {
            intent = intent.passive();
        }
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
        (plan, facts)
    }

    /// Executes `plan` for `input` with the leaf writers.
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
                // Counts and reasons only, never the text.
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

    /// Text: the warpification UX collapses for every key that gets past the ssh holds, then the
    /// pty (with the editor as the refused fallback), the editor, or the agent's input.
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
                    !self.input_is_synced(),
                    ctx,
                );
                warpui::input_order::log_key_hop("kh_terminal_typed_routed", || {
                    format!("written_to_pty={written} table=1")
                });
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
            other => self.table_no_leaf(Replayed::Typed(text), other, plan, ctx),
        }
    }

    /// Text that goes to the editor: the selection, the scroll position and the focus follow it.
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
        // #2181: a typed key the ssh gate held is judged for the line-start ssh escape like a
        // typed key; every other insert is the plain system insert.
        let held_typed = self.paste_queue.delivering_held_typed;
        self.input.update(ctx, |input, ctx| {
            if held_typed {
                input.system_insert_held(text, ctx)
            } else {
                input.system_insert(text, ctx)
            }
        });
        if self.focus_reclaim_active(ctx) {
            self.place_focus_after_card_resolved(ctx);
        }
    }

    /// A held control key or a replayed Enter: where the text before it went (the pty, else the
    /// editor).
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
                    "\u{b}" => EditorAction::DeleteAllRight,
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
            other => self.table_no_leaf(Replayed::Key(key), other, plan, ctx),
        }
    }

    /// A held escape sequence: the pty, or the editor's key after the hand-off took the line over.
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
                // #1661 review: a held Tab the plan sends to the editor runs the editor's Tab
                // (`apply_key_to_editor_after_handoff` maps it).
                if !self.apply_key_to_editor_after_handoff(seq.as_bytes(), ctx) {
                    log::info!("input queue: a held control sequence had no editor action");
                }
            }
            other => self.table_no_leaf(Replayed::Seq(seq), other, plan, ctx),
        }
    }

    /// A held paste: into the input, or to the pty (bracketed as the facts say).
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
            other => self.table_no_leaf(Replayed::Paste(text), other, plan, ctx),
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

    /// An outcome the executors have no leaf for. None is reachable: the table, asked for a
    /// delivery (no gate behind the entry, [`table::delivery_reads_setup_as_announced`]) or for an
    /// arrival after its holds, answers each class with the outcomes its executor handles
    /// (`table_enum_tests.rs`: `every_delivery_plan_has_a_leaf`, `every_arrival_plan_has_a_leaf`).
    /// Until #1353 step 8 this arm re-ran the legacy chain. Now it is an error in the log (the row
    /// and the outcome, never the text) and a leaf that cannot run anything: text and pastes are
    /// put in the input editor, where the user sees them; a control key or a sequence is not
    /// applied.
    fn table_no_leaf(
        &mut self,
        input: Replayed<'_>,
        outcome: Outcome,
        plan: &Plan,
        ctx: &mut ViewContext<Self>,
    ) {
        note_no_leaf();
        log::error!(
            "input queue: no leaf for {outcome:?} (row {}, class {:?}); the input is kept in the \
             editor or not applied, never written",
            plan.row.name(),
            input.class()
        );
        warpui::input_order::log_key_hop("kh_table_no_leaf", || {
            format!("row={} outcome={outcome:?}", plan.row.name())
        });
        match input {
            Replayed::Typed(text) => self.insert_typed_into_editor(text, Effects::NONE, ctx),
            Replayed::Paste(text) => {
                self.input.update(ctx, |input, ctx| {
                    input.paste_insert(text, ctx);
                });
            }
            Replayed::Key(_) | Replayed::Seq(_) | Replayed::Signal(_) => {}
        }
    }
}

// ---- arrival of terminal keys (step 5) ----------------------------------------------------------
//
// The three entry points a live key arrives at (`typed_characters_on_terminal`,
// `keydown_on_terminal`, `control_sequence_on_terminal`) hand the input to
// [`TerminalView::arrive_via_table`]. It takes the stamp first (what the key saw when it reached
// the sink), runs the refreshers a key needs (the subshell gates that open on the first key, the
// stale young Enter hold, the lazy arming of the connect gate for a signal), builds the facts WITH
// the queue's own (open gates, queued entries, the mailbox), decides once and executes the plan
// with the leaf writers. The idle path (nothing held, nothing queued) is the plan `Pty` executed at
// once; a key a gate holds is offered to the queue as destination-neutral content (`Text`, `Enter`,
// or a `Held` key), which the queue's delivery decides again, with the facts of THAT moment, when
// the gate opens (`deliver_via_table`). The owner's decisions are in the table: ^Z and ^\ jump the
// queue like ^C (OD-1), a signal typed in the ssh setup window is held and delivered to the remote
// shell (OD-5), a live Enter while the agent owns the pty is dropped (OD-2).
//
// Not routed through neutral entries: the keys that wait behind the typeahead hand-off's gates
// (`LineEditorActive`, `PromptSeen`, `HandoffReport`) and the PowerShell setup. Those gates give
// their keys back as BYTES (`bytes_of_taken_entries`, the hand-off's own take-and-write), so the
// key is written through the writer (`write_user_bytes_to_pty`), which queues it as bytes.

thread_local! {
    /// The origin an input arriving at an entry point is judged with: a keyboard key unless the
    /// writer of another origin says so (synced input, the paced CLI-agent submit, a shared
    /// session viewer) around its call. Thread-local: the entry points are plain methods with no
    /// room for another argument.
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

/// Whether the input being written arrived as synced input (#1855, #1430): passive in both modes,
/// it never takes over an agent-run command in this pane.
pub(super) fn arrival_is_synced() -> bool {
    matches!(arrival_origin(), Origin::Synced)
}

impl TerminalView {
    /// Whether the input being written is synced input (#1855, #1893): the delivering entry's own
    /// origin while the queue delivers (a drain run inside a synced arrival, or a synced entry
    /// drained from a pty event, must not read the enclosing arrival's origin), else the arrival's.
    pub(super) fn input_is_synced(&self) -> bool {
        if self.paste_queue.delivering {
            self.paste_queue.delivering_synced
        } else {
            arrival_is_synced()
        }
    }

    /// #1900: the origin an input held in the pane queue is offered with. Synced input stays
    /// [`Origin::Synced`] while it waits (behind an ssh connect, behind an Enter's gate), because
    /// the delivery reads the ENTRY's origin to keep a relay passive (`delivering_synced`, #1893):
    /// offered as a keyboard entry it took over an agent-run command when it was delivered.
    /// Everything else is a keyboard entry, as before.
    pub(super) fn held_input_origin(&self) -> Origin {
        held_origin_for(
            self.input_is_synced(),
            crate::terminal::input_queue::switches::keep_synced_origin(),
        )
    }

    /// #1900: the input's `queue_behind_enter` (`input/enter_queue.rs`) for SYNCED input: the text
    /// (or Enter) goes behind the Enter whose gate is open, in arrival order, as an entry that
    /// still says it is synced. The input's own offer has no origin to give (it is the editor's
    /// route for typed keys), so the view offers to the same pane queue itself. `stamp` is the
    /// key's own when the caller has one. False when no Enter gate holds the queue (the caller
    /// goes on, as after `queue_behind_enter`).
    pub(super) fn queue_synced_behind_enter(
        &mut self,
        after: AfterEnterInput,
        stamp: Option<Stamp>,
        ctx: &mut ViewContext<Self>,
    ) -> bool {
        if !self.input.as_ref(ctx).enter_queue_holds_keys(ctx) {
            return false;
        }
        let kind = match after {
            AfterEnterInput::Text(text) => EntryKind::Text(text),
            AfterEnterInput::Enter => EntryKind::Enter,
        };
        let queue = self.paste_queue.queue.clone();
        // Ordered: an offer still in the mailbox was typed first (as `queue_behind_enter`).
        let Some(mut guard) = queue.try_lock_ordered() else {
            queue.defer(Deferred::Offer {
                kind,
                origin: Origin::Synced,
                stamp: stamp.unwrap_or_else(|| queue.stamp_now()),
            });
            return true;
        };
        let mut host = PtyQueueHost::new(self, ctx);
        match stamp {
            Some(stamp) => guard.offer_stamped(&mut host, kind, Origin::Synced, stamp),
            None => guard.offer_with_origin(&mut host, kind, Origin::Synced),
        };
        true
    }
}

/// [`TerminalView::held_input_origin`] as a pure function: `synced` is whether the input being
/// held is synced input, `keep_synced` the kill switch (`PHOSPHOR_QUEUE_KEEP_SYNCED_ORIGIN`).
pub(super) fn held_origin_for(synced: bool, keep_synced: bool) -> Origin {
    if synced && keep_synced {
        Origin::Synced
    } else {
        Origin::Keyboard
    }
}

/// A live input arriving at one of the three entry points.
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
    /// An entry point was called while the queue delivers an entry. No caller does (the entry
    /// points run from event and action handlers, after the update that drains the queue); until
    /// #1353 step 8 the legacy body took such a key and its writes passed as part of the delivery.
    /// The key waits its turn instead: offered to the queue with the stamp and the origin of now,
    /// which the busy queue keeps in its mailbox, in order, and the table decides it when the
    /// drain has ended. Nothing is lost or reordered. Logged (the class, never the text). Not for
    /// a signal: a signal never waits (the caller decides it at once, as any arrival).
    fn arrival_mid_delivery(&mut self, arrival: Arrival<'_>, ctx: &mut ViewContext<Self>) {
        let stamp = self.paste_queue.queue.stamp_now();
        let origin = arrival_origin();
        let class = arrival.class();
        log::warn!("input queue: a key arrived while the queue was delivering; it waits");
        warpui::input_order::log_key_hop("kh_arrival_mid_delivery", || format!("class={class:?}"));
        self.offer_arrival_to_queue(arrival, stamp, origin, ctx);
    }

    /// A live control sequence whose bytes are not text (an 8-bit sequence): decided by the same
    /// rows as [`Arrival::Seq`], written as bytes. The writer (`write_user_bytes_to_pty`) holds
    /// the bytes behind whatever gate is open; the ssh connect does not hold them (it holds
    /// text), as before #1353 step 8.
    pub(super) fn arrive_sequence_bytes_via_table(
        &mut self,
        bytes: &[u8],
        ctx: &mut ViewContext<Self>,
    ) {
        let stamp = self.paste_queue.queue.stamp_now();
        let origin = arrival_origin();
        if self.paste_queue.delivering {
            log::warn!("input queue: a key arrived while the queue was delivering; it waits");
            warpui::input_order::log_key_hop("kh_arrival_mid_delivery", || {
                "class=SeqBytes".to_owned()
            });
            self.paste_queue.queue.defer(Deferred::Offer {
                kind: EntryKind::PtyBytes {
                    bytes: bytes.to_vec(),
                    takes_over_agent_control: true,
                },
                origin,
                stamp,
            });
            return;
        }
        note_table_arrival();
        let _key_path = KeyPathScope::enter(&self.key_path_depth);
        // Operations that found the queue busy come before this key (typed order).
        self.flush_deferred_queue_ops(ctx);
        let queue_facts = self.paste_queue.queue.try_queue_facts().unwrap_or_default();
        let facts = self.build_route_facts(queue_facts, ctx);
        let mut intent = Intent::new(Class::Seq, origin).stamped(stamp);
        if matches!(origin, Origin::Synced) {
            intent = intent.passive();
        }
        let plan = table::decide(&intent, &facts, None);
        warpui::input_order::log_key_hop("kh_table_decide", || {
            format!(
                "class=SeqBytes origin={origin:?} row={} outcome={:?} effects={:?} arrival=1",
                plan.row.name(),
                plan.outcome,
                plan.effects
            )
        });
        match plan.outcome {
            Outcome::EditorAction => {
                if !self.apply_key_to_editor_after_handoff(bytes, ctx) {
                    log::info!("input queue: a control sequence had no editor action");
                }
            }
            Outcome::Drop(why) => log_table_drop(&plan, why),
            // The pty, now or behind the gate the plan named: the writer queues the bytes.
            _ => {
                self.on_ssh_warpification_key_event(Some(SshKeyEvent::from_bytes(bytes)), ctx);
                self.write_user_bytes_to_pty(bytes.to_owned(), ctx);
            }
        }
    }

    /// Decides a live input with the table and executes the plan. See the section comment.
    pub(super) fn arrive_via_table(&mut self, arrival: Arrival<'_>, ctx: &mut ViewContext<Self>) {
        if self.paste_queue.delivering && !matches!(arrival.class(), Class::Signal(_)) {
            self.arrival_mid_delivery(arrival, ctx);
            return;
        }
        note_table_arrival();
        // First: what the key saw when it reached the sink (the focus rows judge a key by it).
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
        // A window released without a hand-over: hand over BEFORE this key is judged, so it lands
        // behind the held keys (typed order, #1348).
        // A signal is not such a key: it ends the hold (the tty flushes what was typed before a
        // ^C, P1b), so it must not first hand the held text back to the editor (9e02279ee).
        if self.ssh_handover_after_release && !is_signal {
            self.deliver_ssh_typeahead_if_ready("released_before_key", ctx);
        }
        // Step 7: no ssh refresher runs on the key path (the connect gate is armed, released and
        // handed over by the events, `view/ssh_edges.rs`); the facts carry the machine's phase.
        if !is_signal {
            // A young Enter's hold the hand-off's state has outgrown ends before this key is
            // judged, and the subshell gates the first key opens are open when the facts are read.
            self.settle_stale_young_hold_before_key(ctx);
            // #1679 step 3 (R8): a secret read opened while the setup is written hands the held
            // keys back BEFORE this key is judged, so it goes to the password. The writer does this
            // in `write_user_bytes_to_pty_inner`; a key the table holds never gets there (Mac round 7:
            // the typed password was held, then ran as part of a command line).
            self.secret_read_on_key_path_cancels_written_setup(ctx);
            let _ = self.prepare_subshell_gates(text.as_bytes(), ctx);
        }
        if is_signal {
            // #1856: the connect gate is armed lazily for a signal; a ^C in the connect window
            // before the ConnectStart edge must still find it (P1e, #1001).
            let _ = self.refresh_ssh_connect_gate();
        }
        // #1857: the per-key entry hop (`kh_terminal_typed_entry`, `kh_hold_entry`), the fields
        // the case oracles read. Counts only, never the text.
        warpui::input_order::log_key_hop("kh_terminal_typed_entry", || {
            format!(
                "chars={} ssh_holding={} gate_armed={} table=1",
                text.chars().count(),
                self.ssh_typeahead_holding,
                self.ssh_connect_gate.is_some()
            )
        });
        let queue_facts = self.paste_queue.queue.try_queue_facts().unwrap_or_default();
        let facts = self.build_route_facts(queue_facts, ctx);
        let mut intent = Intent::new(class, origin).stamped(stamp);
        if matches!(origin, Origin::Synced) {
            // #1855: synced input is passive (it never takes over an agent's command).
            intent = intent.passive();
        }
        let plan = table::decide(&intent, &facts, None);
        warpui::input_order::log_key_hop("kh_table_decide", || {
            format!(
                "class={class:?} origin={origin:?} row={} outcome={:?} effects={:?} arrival=1",
                plan.row.name(),
                plan.outcome,
                plan.effects
            )
        });
        // #1857: the hold hops (`kh_hold_entry`, `kh_hold_exit`), from the facts the plan was
        // decided on.
        let pending_session = self.model_facts().pending_session_id.is_some();
        warpui::input_order::log_key_hop("kh_hold_entry", || {
            format!(
                "holding={} held_bytes={} gate_armed={} pending_session={} table=1",
                self.ssh_typeahead_holding,
                self.ssh_held_bytes(),
                self.ssh_connect_gate.is_some(),
                pending_session
            )
        });
        let held = matches!(
            plan.outcome,
            Outcome::Hold(_) | Outcome::HoldSignal | Outcome::Queue
        );
        warpui::input_order::log_key_hop("kh_hold_exit", || {
            if held {
                format!("branch=held held_bytes={} table=1", self.ssh_held_bytes())
            } else {
                format!(
                    "branch=gate_open_live gate_armed={} table=1",
                    self.ssh_connect_gate.is_some()
                )
            }
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
                    // #1900: synced input keeps its origin behind the Enter's gate.
                    let queued = if self.held_input_origin() == Origin::Synced {
                        self.queue_synced_behind_enter(after, Some(stamp), ctx)
                    } else {
                        self.input
                            .update(ctx, |input, ctx| input.queue_behind_enter(after, ctx))
                    };
                    if queued {
                        return;
                    }
                }
                self.offer_arrival_to_queue(arrival, stamp, origin, ctx);
            }
            // The hand-off's gates and the PowerShell setup give their keys back as bytes: the key
            // is written through the writer, which queues it as bytes behind them.
            GateKind::LineEditorActive
            | GateKind::PromptSeen
            | GateKind::HandoffReport
            | GateKind::PowershellSetup => self.hold_arrival_as_bytes(arrival, ctx),
            _ => self.offer_arrival_to_queue(arrival, stamp, origin, ctx),
        }
    }

    /// The key goes through the writer (`write_user_bytes_to_pty`), which holds it behind the
    /// gate the table named.
    fn hold_arrival_as_bytes(&mut self, arrival: Arrival<'_>, ctx: &mut ViewContext<Self>) {
        self.note_arrival_for_warpification(arrival, ctx);
        self.highlighted_link.invalidate();
        if !matches!(arrival, Arrival::Seq(_)) {
            self.report_possible_typeahead(arrival.text());
        }
        let bytes = arrival.text().as_bytes().to_vec();
        if self.input_is_synced() {
            self.write_passive_input_to_pty(bytes, ctx);
        } else {
            self.write_user_bytes_to_pty(bytes, ctx);
        }
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
                // Not reachable (`table_enum_tests.rs`, `every_arrival_plan_has_a_leaf`): a live
                // control key gets the pty, the editor, a hold or a drop. As a key nothing takes.
                note_no_leaf();
                log::error!(
                    "input queue: an arriving key has no leaf for {other:?} (row {}); not applied",
                    plan.row.name()
                );
                warpui::input_order::log_key_hop("kh_table_no_leaf", || {
                    format!("row={} outcome={other:?} arrival=1", plan.row.name())
                });
                self.refocus_after_dropped_key(ctx);
            }
        }
    }

    /// A signal (^C ^D ^Z ^\) typed or pressed: held in the ssh setup window and delivered to the
    /// remote shell (P1a, OD-5), or sent on through the writer, which applies the queue's
    /// per-gate rule (`signal_plan`) and jumps the keys held in front of it.
    fn execute_arrival_signal(
        &mut self,
        text: &str,
        signal: SignalKind,
        plan: &Plan,
        ctx: &mut ViewContext<Self>,
    ) {
        // #2134 (owner decision 2026-11-11): row P1a holds an interrupt while the remote
        // bootstrap is pending. With the app's own remote-helper question up for that session
        // there is no bootstrap being fed, and the interrupt ends the ssh instead (the same
        // rule as `ctrl_c_to_active_block`). A ^D stays held.
        if plan.outcome == Outcome::HoldSignal
            && signal == SignalKind::Interrupt
            && let Some(question_session) = self.helper_question_ended_by_interrupt(ctx)
        {
            self.end_ssh_at_helper_question(question_session, ctx);
            return;
        }
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
        // An Enter that waits for its verdict is void -- for ^C / ^D only
        // (`is_abort_key`). ^Z / ^\ jump the queue (OD-1) but the held Enter and the keys behind
        // it stay (#1852): the tty does not read them as an interrupt of the line.
        if matches!(signal, SignalKind::Interrupt | SignalKind::Eof)
            && self.input.as_ref(ctx).enter_queue_holds_keys(ctx)
        {
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

// ---- the rows of the writers of another origin (steps 5 and 6) ----------------------------------

/// What a writer of another origin (synced input, the paced CLI-agent submit, a shared-session
/// viewer, the agent) learned from the table before it wrote.
pub(super) struct OriginRow {
    refused: bool,
}

impl OriginRow {
    /// The table drops the input at its origin row (`O-synced`, `D13`): the writer writes nothing.
    pub(super) fn refused(&self) -> bool {
        self.refused
    }
}

impl TerminalView {
    /// Starts a writer of `origin`: the table decides the input (`class`, `origin`) and a drop at
    /// the origin row refuses the write (the other rows are what the writers' stages apply
    /// themselves; the plan is logged).
    pub(super) fn begin_origin_row(
        &mut self,
        class: Class,
        origin: Origin,
        ctx: &mut ViewContext<Self>,
    ) -> OriginRow {
        let _origin = set_arrival_origin(origin);
        let stamp = self.paste_queue.queue.stamp_now();
        // The connect gate is armed lazily from the warpify state before it is read
        // (`agent_pty_write_gate` reads it too).
        let _ = self.refresh_ssh_connect_gate();
        if self.ssh_handover_after_release {
            // Next turn, not here: an agent write is not a user key and must not drain the user's
            // queue from inside its own path.
            ctx.spawn(async {}, |me, _, ctx| {
                me.deliver_ssh_typeahead_if_ready("released_by_origin_write", ctx);
            });
        }
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
            // The rows that refuse a write at its origin: the synced origin (no running command)
            // and the agent's write (a setup owns the pty).
            refused: matches!(plan.outcome, Outcome::Drop(_))
                && matches!(plan.row, Row::OSynced | Row::D13),
        }
    }

    /// Whether a paste made now goes into the input (else to the pty) and whether the pty paste is
    /// bracketed, as the table's plan says: the shape of `paste_routing`, which the tests compare
    /// it with (`deliver_paste_text` and the give-up writers still read that predicate).
    #[cfg(test)]
    pub(super) fn paste_routing_via_table(&mut self, ctx: &mut ViewContext<Self>) -> (bool, bool) {
        let (plan, bracketed) = self.paste_plan_via_table(ctx);
        (
            matches!(plan.outcome, Outcome::EditorInsert | Outcome::AgentInput),
            bracketed,
        )
    }

    /// The table's whole plan for a paste made now (#1853), and whether a pty paste is bracketed.
    /// `paste()` branches on the outcome: an ssh hold, the setup dialog's swallow, the choice
    /// card's hint and a drop are not pty pastes. The destination rows only (D1, D2, D5-D7): the
    /// paste read takes its own place in the queue (a `PasteReading` gate), so no queue fact is
    /// passed, and nothing blocks on the model's lock.
    pub(super) fn paste_plan_via_table(&mut self, ctx: &mut ViewContext<Self>) -> (Plan, bool) {
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
        (plan, facts.needs_bracketed_paste)
    }
}

// ---- focus changes and the line pin (step 7) ----------------------------------------------------

impl TerminalView {
    /// Focus moved (the terminal view gained or lost it, or the input editor did): the queue's
    /// focus epoch advances and a head that waited on destination facts is drained. A focus flip
    /// does NOT clear the line pin (it never splits a line). When the queue is busy the change is
    /// recorded as a deferred operation, applied in order.
    pub(super) fn note_queue_focus_changed(&mut self, ctx: &mut ViewContext<Self>) {
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
        self.paste_queue.queue.route_state().request_pin_clear();
    }
}
