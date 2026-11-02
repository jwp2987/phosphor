//! The ssh connect typeahead hold on the one input queue (#1353 phase 4).
//!
//! Between the Enter on an `ssh ...` command and the remote shell's first prompt, keys typed into
//! the pane are not written anywhere (#983, #1292). They are `Held` entries of the pane's
//! `InputQueue` behind an `SshConnect` gate, whichever route typed them: the terminal element
//! offers them through [`TerminalView::ssh_hold_push`], and the input editor offers its own
//! (origin: the editor) straight to the shared queue while the gate is open
//! (`EditorView::offer_ssh_held_key`), so arrival order is the only order there is. The queue
//! only orders; WHERE a key goes is decided when it is delivered, by the same routing a key typed
//! at that moment would take (editor after the hand-over, pty for a prompt that wants an answer).
//!
//! Events, all of them the existing ones (#1348, none of them a timer):
//! - the gate OPENS with the first key that must be held (the stretch between the Enter and that
//!   key has nothing to hold);
//! - it CLOSES at the single hand-over point (`deliver_ssh_typeahead_if_ready`, driven by the
//!   remote shell's first Precmd and its siblings): the close IS the delivery, the queue's one
//!   drain writes the held keys in typed order;
//! - a host-key / password prompt in the output, or a ^C, DROPS the gate with everything held
//!   (`reset_ssh_typeahead`), and every later key passes straight to the prompt;
//! - a connect that ended without a remote shell turns the held keys into editor text, never
//!   run (`finish_ssh_typeahead`).
//! The only timer left is the give-up deadline of `schedule_ssh_typeahead_give_up` (its deadline
//! depends on the connect gate's state, so the queue gate itself carries no cap).
//!
//! A key that must join the hold while the queue is mid-delivery (text replayed behind an Enter
//! whose command opened the connect, a held input judged at a hand-over for the next connect)
//! cannot be offered: `ssh_hold_requeue` asks the host to put it back at the front under an
//! `SshConnect` gate when the delivery returns, as the hand-off's holds do (#1415). The hand-over
//! stops its drain after each held Enter (`Host::yield_after`) and continues on the next turn, so
//! the connect gate armed by a command that Enter submitted is in place before the next held input
//! is judged (#1421).
//!
//! The 64 KiB bound still drops the WHOLE run and ignores the rest of that connect.

use warpui::ViewContext;

use super::TerminalView;
use super::paste_queue::PtyQueueHost;
use super::ssh_typeahead::{HeldInput, SSH_TYPEAHEAD_MAX_BYTES, SshTypeahead, SshTypeaheadPush};
use crate::terminal::input_queue::{
    Deferred, EntryKind, GateKind, GateSpec, Origin, SshDropThen, trace,
};

/// What the ssh hold keeps next to the queue itself: which gate is open. What it holds is read
/// from the queue (entries of both origins).
pub(super) struct SshQueueState {
    /// The key (the connection counter) of the open `SshConnect` gate.
    pub(super) gate: Option<u64>,
    next_conn: u64,
    /// The 64 KiB bound was passed: the run was dropped and the rest of this connect is ignored.
    overflowed: bool,
}

impl SshQueueState {
    pub(super) fn new() -> Self {
        Self {
            gate: None,
            next_conn: 0,
            overflowed: false,
        }
    }
}

impl TerminalView {
    /// Entries and content bytes held behind the connect gate, of every origin (nothing while the
    /// queue is mid-delivery: what a caller would read is in flux).
    fn ssh_held_stats(&self) -> (usize, usize) {
        self.paste_queue.queue.try_peek().map_or((0, 0), |queue| {
            queue.queued_since_gate(GateKind::SshConnect)
        })
    }

    /// Nothing is held (an overflowed run holds nothing).
    pub(super) fn ssh_held_is_empty(&self) -> bool {
        self.ssh_held_stats().0 == 0
    }

    /// Bytes held; for logging, never the content.
    pub(super) fn ssh_held_bytes(&self) -> usize {
        self.ssh_held_stats().1
    }

    /// Opens the connect gate if none is open: keys (and pastes) offered from now on wait behind
    /// it. The gate may have been closed behind the view's back (a signal ends a gate): never
    /// offer to a queue with no gate, it would be delivered at once, mid-connect.
    pub(super) fn ssh_hold_ensure_gate(&mut self, ctx: &mut ViewContext<Self>) {
        let queue = self.paste_queue.queue.clone();
        let gate_alive = self.ssh_iq.gate.is_some_and(|key| {
            // A gate whose opening waits in the mailbox is alive too.
            queue.has_deferred()
                || queue
                    .try_peek()
                    .is_some_and(|queue| queue.has_gate(GateKind::SshConnect, key))
        });
        if gate_alive {
            return;
        }
        self.ssh_iq.next_conn += 1;
        let key = self.ssh_iq.next_conn;
        let Some(mut guard) = queue.try_lock() else {
            // Only reachable if a caller skipped the busy check `ssh_hold_push` makes first; the
            // gate then opens with the next operation and the entry that follows it is deferred
            // behind it, in order. The token is learnt then (the gate is looked up by key).
            queue.defer(Deferred::OpenGate(
                GateSpec::new(GateKind::SshConnect, key).with_cap(None),
            ));
            self.ssh_iq.gate = Some(key);
            return;
        };
        let mut host = PtyQueueHost::new(self, ctx);
        guard.open_gate(
            &mut host,
            GateSpec::new(GateKind::SshConnect, key).with_cap(None),
        );
        self.ssh_iq.gate = Some(key);
    }

    /// Holds one input behind the connect, in typed order.
    pub(super) fn ssh_hold_push(
        &mut self,
        input: HeldInput,
        ctx: &mut ViewContext<Self>,
    ) -> SshTypeaheadPush {
        if self.ssh_iq.overflowed {
            return SshTypeaheadPush::Ignored;
        }
        if self.paste_queue.queue.try_peek().is_none() {
            // Mid-delivery: the drain that is running owns the queue, so the input cannot be
            // offered. It goes back to the front of the queue under an `SshConnect` gate when
            // the delivery returns, the way the hand-off's holds do (#1415): a key replayed
            // behind an Enter whose command opened a connect, or one held at the hand-over for
            // the next connect, is held, never dropped.
            if self.paste_queue.delivering {
                return self.ssh_hold_requeue(input);
            }
            log::warn!("ssh hold: input queue busy outside a delivery, a key was not held");
            return SshTypeaheadPush::Ignored;
        }
        let len = input.byte_len();
        if self.ssh_held_bytes() + len > SSH_TYPEAHEAD_MAX_BYTES {
            return self.ssh_hold_overflow();
        }
        self.ssh_hold_ensure_gate(ctx);
        let queue = self.paste_queue.queue.clone();
        let (class, text) = input.into_parts();
        trace::hold(trace::legacy::SSH, len);
        let kind = EntryKind::Held { class, text };
        let Some(mut guard) = queue.try_lock() else {
            // Held in order by the next operation (never dropped, never written as typed).
            queue.defer(Deferred::Offer {
                kind,
                origin: Origin::Keyboard,
                stamp: queue.stamp_now(),
            });
            return SshTypeaheadPush::Held;
        };
        let mut host = PtyQueueHost::new(self, ctx);
        guard.offer(&mut host, kind);
        SshTypeaheadPush::Held
    }

    /// Holds `input`, the key the running delivery routed here, by asking the host to put it
    /// back at the front of the queue under the connect gate (the same `Requeue` the hand-off's
    /// holds use). The gate is the open one when there is one (its key is closed together with
    /// every gate of that key at the hand-over), else a new connection's.
    fn ssh_hold_requeue(&mut self, input: HeldInput) -> SshTypeaheadPush {
        let key = match self.ssh_iq.gate {
            Some(key) => key,
            None => {
                self.ssh_iq.next_conn += 1;
                let key = self.ssh_iq.next_conn;
                self.ssh_iq.gate = Some(key);
                key
            }
        };
        let (class, text) = input.into_parts();
        trace::hold(trace::legacy::SSH, text.len());
        let spec = GateSpec::new(GateKind::SshConnect, key).with_cap(None);
        let held = EntryKind::Held { class, text };
        if let Err(held) = self.paste_queue.request_requeue_held(spec, held) {
            // A second input of another class in one delivery cannot join the first: it is
            // recorded to go behind the same gate right after the requeue, in typed order (the
            // busy-queue mailbox), instead of being dropped.
            self.paste_queue
                .queue
                .defer(Deferred::OfferGated { kind: held, spec });
        }
        SshTypeaheadPush::Held
    }

    /// Gives up the whole run as if it had grown past its bound.
    pub(super) fn ssh_hold_overflow(&mut self) -> SshTypeaheadPush {
        if self.ssh_iq.overflowed {
            return SshTypeaheadPush::Ignored;
        }
        self.ssh_hold_clear_gate(SshDropThen::Discard, None);
        self.ssh_iq.overflowed = true;
        SshTypeaheadPush::Overflowed
    }

    /// Detaches the view's state of the hold (the gate key and the pastes still reading behind
    /// it). The queue's entries are dropped separately ([`Self::ssh_drop_detached`]), now or as a
    /// deferred operation when the queue is busy, so a busy queue never leaves the gate set.
    fn ssh_hold_detach(&mut self) -> (Option<u64>, Vec<u64>) {
        let gate = self.ssh_iq.gate.take();
        let paste_ids: Vec<u64> = self.paste_queue.ssh_pastes.drain().collect();
        (gate, paste_ids)
    }

    /// Removes the gate `gate` and everything behind it without delivering, and returns what it
    /// held in typed order. That includes the pastes whose clipboard read is still pending behind
    /// the connect (their gates and what was typed after them): they were typed for a session that
    /// is not coming (a prompt appeared, ^C, a new command), so their late answer must not be
    /// pasted anywhere. A paste that was already read comes back as a held paste. The queue must
    /// not be busy (callers check, or run this from the mailbox).
    pub(super) fn ssh_drop_detached(
        &mut self,
        gate: Option<u64>,
        paste_ids: &[u64],
    ) -> Vec<HeldInput> {
        if gate.is_none() && paste_ids.is_empty() {
            return Vec::new();
        }
        let mut removed = Vec::new();
        match self.paste_queue.queue.try_lock() {
            Some(mut queue) => {
                if let Some(key) = gate {
                    removed.extend(queue.drop_gate(GateKind::SshConnect, key));
                }
                for id in paste_ids {
                    removed.extend(queue.drop_gate(GateKind::PasteReading, *id));
                }
            }
            None => {
                // Not reachable from the callers (they check, the mailbox applies only when
                // free). Never lose the drop: it is recorded to run when the queue is free.
                debug_assert!(false, "ssh hold: queue busy in ssh_drop_detached");
                log::error!("ssh hold: input queue busy while dropping the ssh gate, deferred");
                self.paste_queue.queue.defer(Deferred::DropSshGate {
                    gate,
                    paste_ids: paste_ids.to_vec(),
                    then: SshDropThen::Discard,
                });
                return Vec::new();
            }
        }
        removed.sort_by_key(|entry| entry.id);
        if !removed.is_empty() {
            warpui::input_order::log_key_hop("kh_iq_discard", || {
                format!("queue={} entries={}", trace::legacy::SSH, removed.len())
            });
        }
        let mut held = Vec::new();
        for entry in removed {
            match entry.kind {
                EntryKind::Held { class, text } => held.push(HeldInput::from_parts(class, text)),
                EntryKind::Paste { paste_id, .. } => {
                    self.paste_queue.routed_pastes.remove(&paste_id);
                    self.paste_queue.refused.remove(&paste_id);
                    if let Some(bytes) = self.paste_queue.payloads.remove(&paste_id) {
                        held.push(HeldInput::Paste(
                            String::from_utf8_lossy(&bytes).into_owned(),
                        ));
                    }
                }
                _ => {}
            }
        }
        held
    }

    /// Drops the hold's gate now, or (the queue is busy: mid-drain, or earlier operations wait in
    /// the mailbox) records the drop as a deferred operation that carries `then` (#1446). The
    /// view's state is detached either way, so the hold reads as over at once. With `ctx`, a
    /// pending drop is also applied right away when only the mailbox was in the way.
    fn ssh_hold_clear_gate(&mut self, then: SshDropThen, ctx: Option<&mut ViewContext<Self>>) {
        let (gate, paste_ids) = self.ssh_hold_detach();
        if gate.is_none() && paste_ids.is_empty() {
            return;
        }
        let queue = self.paste_queue.queue.clone();
        if let Some(ctx) = ctx {
            if !queue.is_draining() {
                self.flush_deferred_queue_ops(ctx);
            }
            if !queue.has_deferred() && queue.try_peek().is_some() {
                let held = self.ssh_drop_detached(gate, &paste_ids);
                self.finish_dropped_ssh_gate(held, then, ctx);
                return;
            }
        } else if !queue.has_deferred() && queue.try_peek().is_some() {
            // Discarding needs no view context.
            debug_assert_eq!(then, SshDropThen::Discard);
            let _ = self.ssh_drop_detached(gate, &paste_ids);
            return;
        }
        queue.defer(Deferred::DropSshGate {
            gate,
            paste_ids,
            then,
        });
    }

    /// The continuation of a dropped ssh gate: what it held is forgotten, or shown in the editor
    /// as unexecuted text.
    pub(super) fn finish_dropped_ssh_gate(
        &mut self,
        held: Vec<HeldInput>,
        then: SshDropThen,
        ctx: &mut ViewContext<Self>,
    ) {
        match then {
            SshDropThen::Discard => {}
            SshDropThen::HandBackText => {
                // Adjacent printable runs are one input.
                let mut merged = SshTypeahead::default();
                for input in held {
                    merged.push(input);
                }
                let mut collector = SshTypeahead::default();
                for input in merged.take() {
                    collector.push(input);
                }
                let text = collector.unexecuted_text();
                self.hand_back_unexecuted_ssh_text(text, ctx);
            }
        }
    }

    /// Takes everything held, in typed order, without delivering it (tests; the queue is free).
    #[cfg(test)]
    pub(super) fn ssh_hold_take(&mut self) -> Vec<HeldInput> {
        let (gate, paste_ids) = self.ssh_hold_detach();
        let held = self.ssh_drop_detached(gate, &paste_ids);
        let mut merged = SshTypeahead::default();
        for input in held {
            merged.push(input);
        }
        merged.take()
    }

    /// Forgets every held key and the overflow state.
    pub(super) fn ssh_hold_clear(&mut self) {
        self.ssh_hold_clear_gate(SshDropThen::Discard, None);
        self.ssh_iq.overflowed = false;
    }

    /// The connect ended without a remote shell: what the held keys would show in an empty editor,
    /// without running anything, is handed back to the editor (consumes the hold). When the queue
    /// is busy this is a deferred operation, so the text is never lost.
    pub(super) fn ssh_hold_hand_back_unexecuted_text(&mut self, ctx: &mut ViewContext<Self>) {
        self.ssh_hold_clear_gate(SshDropThen::HandBackText, Some(ctx));
    }

    /// Number of inputs held (for the log line, never content).
    pub(super) fn ssh_held_count(&self) -> usize {
        self.ssh_held_stats().0
    }

    /// The hand-over: closes the gate, which is the delivery. The queue's one drain writes the
    /// held inputs in typed order, each routed from the facts of its own delivery.
    pub(super) fn ssh_hold_deliver(&mut self, ctx: &mut ViewContext<Self>) {
        let Some(key) = self.ssh_iq.gate.take() else {
            return;
        };
        let (items, bytes) = self.ssh_held_stats();
        trace::release(trace::legacy::SSH, items, bytes, "remote");
        let queue = self.paste_queue.queue.clone();
        let Some(mut guard) = queue.try_lock() else {
            // The hand-over happens with the next operation (the gate event is recorded, not lost).
            queue.defer(Deferred::GateEvent {
                kind: GateKind::SshConnect,
                key,
            });
            return;
        };
        let mut host = PtyQueueHost::new(self, ctx);
        guard.on_gate_event(&mut host, GateKind::SshConnect, key);
    }

    /// Delivers one held input the way the same key typed now would be routed.
    pub(super) fn deliver_held_input(&mut self, input: HeldInput, ctx: &mut ViewContext<Self>) {
        match input {
            HeldInput::Typed(text) => self.typed_characters_on_terminal(&text, ctx),
            HeldInput::Key(key) => self.replay_held_key(&key, ctx),
            HeldInput::Paste(text) => self.deliver_paste_text(&text, ctx),
            HeldInput::Seq(seq) => self.control_sequence_on_terminal(seq.as_bytes(), ctx),
        }
    }

    /// Host side of the drain: one `Held` entry reaches its destination.
    pub(super) fn deliver_ssh_held(&mut self, class: u8, text: &str, ctx: &mut ViewContext<Self>) {
        self.deliver_held_input(HeldInput::from_parts(class, text.to_owned()), ctx);
    }

    /// Whether the only open queue gates are ssh ones: bytes that reach the pty writer then are
    /// not held behind them. The connect's keys are held by the hold itself (as `Held` entries,
    /// before they get to the writer); what reaches the writer is a deliberate pass-through (an
    /// answer to a prompt after the hold was reset) or passive traffic.
    pub(super) fn only_ssh_gates_open(&self) -> bool {
        self.paste_queue.queue.try_peek().is_some_and(|queue| {
            !queue.gates().is_empty()
                && queue
                    .gates()
                    .iter()
                    .all(|gate| gate.kind == GateKind::SshConnect)
        })
    }
}
