//! The history-recall gate of the Up arrow on a long history (#2107).
//!
//! A history of more than `INLINE_BUILD_MAX_ENTRIES` entries is listed off-thread, so the inline
//! history menu the Up arrow opens has no rows for a moment, and the recalled line enters the
//! input buffer only when the list lands and its first row is selected. Keys typed in between
//! edited the EMPTY buffer and the recalled line then replaced them (`t246_uu59vu93` + BackSpaces
//! + `e9z` came out as the bare recalled line). The menu's own `KeyGate` holds only Up, Down and
//! Accept, never an edit.
//!
//! The fix is one gate on the pane's existing queue, [`GateKind::HistoryRecall`]. The menu says
//! when its list is pending (`InlineHistoryMenuEvent::ListPending`); the gate opens then, and the
//! queue holds every editor key behind it, in order, as it holds keys behind an Enter. The named
//! event that closes it is the first row selecting the recalled line (`SelectCommand`,
//! `SelectAIPrompt`, `SelectConversation`), the list found empty (`NoResults`), or the menu
//! leaving its mode (`Close`, Escape, an accept, a teardown). The drain then replays the held keys
//! in order, on the recalled line. The one timer is the give-up net of the gate (just above the
//! mixer's own 120 s limit): it closes the menu and replays the keys on the unchanged buffer.
//!
//! ^C meeting the gate: with no Enter held ahead of it the recall is cancelled (the edits typed
//! meanwhile are discarded, as a shell's ^C discards the line) and the ^C acts at once; with an
//! Enter held ahead it waits in order behind it (`editor/view/mod.rs`).
//!
//! `PHOSPHOR_HOLD_KEYS_BEHIND_RECALL=0` never opens the gate: today's behaviour exactly.

use warpui::{AppContext, ViewContext};

use super::paste_queue::InputQueueHost;
use super::{Input, InputSuggestionsMode};
use crate::terminal::input_queue::{Deferred, GateKind, GateSpec, GateToken, switches};

impl Input {
    /// The menu opened with its list still being built: closes the queue on the recall gate so
    /// the keys typed before the recalled line is in the buffer wait for it.
    pub(super) fn open_history_recall_gate(&mut self, ctx: &mut ViewContext<Self>) {
        if !switches::hold_keys_behind_recall() {
            return;
        }
        if !self
            .suggestions_mode_model
            .as_ref(ctx)
            .is_inline_history_menu()
        {
            // The menu was dismissed before this event arrived: nothing to wait for.
            return;
        }
        let queue = self.editor.as_ref(ctx).paste_hold().queue();
        if let Some(key) = self.history_recall_gate {
            if queue
                .try_peek()
                .is_some_and(|held| held.has_gate(GateKind::HistoryRecall, key))
            {
                return;
            }
        }
        self.history_recall_seq = self.history_recall_seq.wrapping_add(1);
        let key = self.history_recall_seq;
        let spec = GateSpec::new(GateKind::HistoryRecall, key);
        match queue.try_lock_ordered() {
            Some(mut guard) => {
                let mut host = InputQueueHost::new(self, ctx);
                guard.open_gate(&mut host, spec);
            }
            // Opened by the next operation, in order (the busy-queue mailbox).
            None => queue.defer(Deferred::OpenGate(spec)),
        }
        self.history_recall_gate = Some(key);
        warpui::input_order::log_hop("history_recall_gate", || {
            format!("open key={key} cap_s={}", 125)
        });
    }

    /// The named event of the gate happened (`why` says which): closes it, and the drain replays
    /// the held keys in order. A no-op when no recall gate is open (the common case).
    pub(super) fn release_history_recall(
        &mut self,
        why: &'static str,
        ctx: &mut ViewContext<Self>,
    ) {
        let Some(key) = self.history_recall_gate.take() else {
            return;
        };
        warpui::input_order::log_hop("history_recall_gate", || {
            format!("release key={key} why={why}")
        });
        let queue = self.editor.as_ref(ctx).paste_hold().queue();
        let Some(mut guard) = queue.try_lock_ordered() else {
            queue.defer(Deferred::GateEvent {
                kind: GateKind::HistoryRecall,
                key,
            });
            queue.defer(Deferred::FlushProgrammaticEdits);
            return;
        };
        let mut host = InputQueueHost::new(self, ctx);
        guard.on_gate_event(&mut host, GateKind::HistoryRecall, key);
        drop(guard);
        self.flush_deferred_programmatic_edits(ctx);
    }

    /// The give-up net of the recall gate fired (`token`): the list's event never came. Closes the
    /// menu first (no buffer restore: the menu never touched the buffer), then the queue replays
    /// the held keys in order on the unchanged buffer. None dropped, none reordered.
    pub(crate) fn give_up_history_recall(&mut self, token: GateToken, ctx: &mut ViewContext<Self>) {
        log::warn!(
            "input queue: the history recall gate never closed (the menu's list did not land); \
             closing the menu and replaying the held keys in order"
        );
        self.history_recall_gate = None;
        if self
            .suggestions_mode_model
            .as_ref(ctx)
            .is_inline_history_menu()
        {
            self.suggestions_mode_model.update(ctx, |model, ctx| {
                model.set_mode(InputSuggestionsMode::Closed, ctx);
            });
        }
        let queue = self.editor.as_ref(ctx).paste_hold().queue();
        let Some(mut guard) = queue.try_lock() else {
            queue.defer(Deferred::GiveUpCap(token));
            return;
        };
        let mut host = InputQueueHost::new(self, ctx);
        guard.give_up(&mut host, token);
        drop(guard);
        self.flush_deferred_programmatic_edits(ctx);
    }

    /// Whether `token` names an open history-recall gate on this input's queue.
    pub(crate) fn is_history_recall_token(&self, token: GateToken, ctx: &AppContext) -> bool {
        self.editor
            .as_ref(ctx)
            .paste_hold()
            .queue()
            .try_peek()
            .is_some_and(|held| {
                held.gates()
                    .iter()
                    .any(|gate| gate.token == token && gate.kind == GateKind::HistoryRecall)
            })
    }
}
