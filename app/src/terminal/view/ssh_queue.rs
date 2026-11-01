//! The ssh connect typeahead hold on the one input queue (#1353 phase 4).
//!
//! Between the Enter on an `ssh ...` command and the remote shell's first prompt, keys typed into
//! the pane are not written anywhere (#983, #1292). They used to sit in `SshTypeahead`; now they
//! are `Held` entries of the pane's [`InputQueue`] (the instance paste and the subshell holds
//! already use) behind an `SshConnect` gate. The queue only orders; WHERE a key goes is decided
//! when it is delivered, by the same routing a key typed at that moment would take (editor after
//! the hand-over, pty for a prompt that wants an answer).
//!
//! Events, all of them the existing ones (#1348, none of them a timer):
//! - the gate OPENS with the first key that must be held (the legacy `ssh_typeahead_holding`
//!   starts there; the stretch between the Enter and that key has nothing to hold);
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
//! The 64 KiB bound still drops the WHOLE run and ignores the rest of that connect.
//! `PHOSPHOR_INPUT_QUEUE_SSH=0` keeps the legacy `SshTypeahead` buffer, which stays in the tree
//! and is also where a key held while the queue is mid-delivery lands.

use std::rc::Rc;

use warpui::ViewContext;

use super::TerminalView;
use super::paste_queue::PtyQueueHost;
use super::ssh_typeahead::{HeldInput, SSH_TYPEAHEAD_MAX_BYTES, SshTypeahead, SshTypeaheadPush};
use crate::terminal::input_queue::kill_switch::{self, Area};
use crate::terminal::input_queue::{EntryKind, GateKind, GateSpec, GateToken, trace};

/// What the ssh hold keeps next to the queue itself: which gate is open and how much it holds.
pub(super) struct SshQueueState {
    /// `PHOSPHOR_INPUT_QUEUE_SSH`; false keeps the legacy buffer.
    pub(super) enabled: bool,
    /// The open `SshConnect` gate: its token and its key (the connection counter).
    gate: Option<(GateToken, u64)>,
    next_conn: u64,
    /// Entries and content bytes held behind the gate.
    items: usize,
    bytes: usize,
    /// The 64 KiB bound was passed: the run was dropped and the rest of this connect is ignored.
    overflowed: bool,
}

impl SshQueueState {
    pub(super) fn new() -> Self {
        Self {
            enabled: kill_switch::is_enabled(Area::Ssh),
            gate: None,
            next_conn: 0,
            items: 0,
            bytes: 0,
            overflowed: false,
        }
    }
}

impl TerminalView {
    /// Held keys live on the queue right now: the switch is on and the queue is not in the
    /// middle of delivering (a key held then cannot take a new entry and uses the legacy buffer).
    fn ssh_queue_usable(&self) -> bool {
        self.ssh_iq.enabled && self.paste_queue.queue.try_borrow().is_ok()
    }

    /// Nothing is held (an overflowed run holds nothing).
    pub(super) fn ssh_held_is_empty(&self) -> bool {
        self.ssh_iq.items == 0 && self.ssh_typeahead.is_empty()
    }

    /// Bytes held; for logging, never the content.
    pub(super) fn ssh_held_bytes(&self) -> usize {
        self.ssh_iq.bytes + self.ssh_typeahead.len_bytes()
    }

    /// Holds one input behind the connect, in typed order.
    pub(super) fn ssh_hold_push(
        &mut self,
        input: HeldInput,
        ctx: &mut ViewContext<Self>,
    ) -> SshTypeaheadPush {
        if !self.ssh_queue_usable() {
            return self.ssh_typeahead.push(input);
        }
        if self.ssh_iq.overflowed {
            return SshTypeaheadPush::Ignored;
        }
        let len = input.byte_len();
        if self.ssh_held_bytes() + len > SSH_TYPEAHEAD_MAX_BYTES {
            return self.ssh_hold_overflow();
        }
        let queue = Rc::clone(&self.paste_queue.queue);
        if self.ssh_iq.gate.is_none() {
            self.ssh_iq.next_conn += 1;
            let key = self.ssh_iq.next_conn;
            let mut host = PtyQueueHost::new(self, ctx);
            let token = queue.borrow_mut().open_gate(
                &mut host,
                GateSpec::new(GateKind::SshConnect, key).with_cap(None),
            );
            self.ssh_iq.gate = Some((token, key));
        }
        let (class, text) = input.into_parts();
        self.ssh_iq.items += 1;
        self.ssh_iq.bytes += len;
        trace::hold(trace::legacy::SSH, len);
        let mut host = PtyQueueHost::new(self, ctx);
        queue
            .borrow_mut()
            .offer(&mut host, EntryKind::Held { class, text });
        SshTypeaheadPush::Held
    }

    /// Gives up the whole run as if it had grown past its bound.
    pub(super) fn ssh_hold_overflow(&mut self) -> SshTypeaheadPush {
        if self.ssh_iq.overflowed {
            return SshTypeaheadPush::Ignored;
        }
        self.ssh_hold_drop_gate();
        self.ssh_iq.items = 0;
        self.ssh_iq.bytes = 0;
        self.ssh_iq.overflowed = true;
        self.ssh_typeahead.overflow()
    }

    /// Removes the gate and everything behind it without delivering (the entries are `Held`
    /// ones, which own no resources).
    fn ssh_hold_drop_gate(&mut self) -> Vec<HeldInput> {
        let Some((_, key)) = self.ssh_iq.gate.take() else {
            return Vec::new();
        };
        match self.paste_queue.queue.try_borrow_mut() {
            Ok(mut queue) => {
                let removed = queue.drop_gate(GateKind::SshConnect, key);
                if !removed.is_empty() {
                    warpui::input_order::log_key_hop("kh_iq_discard", || {
                        format!("queue={} entries={}", trace::legacy::SSH, removed.len())
                    });
                }
                removed
                    .into_iter()
                    .filter_map(|entry| match entry.kind {
                        EntryKind::Held { class, text } => Some(HeldInput::from_parts(class, text)),
                        _ => None,
                    })
                    .collect()
            }
            Err(_) => {
                log::warn!("ssh hold: input queue busy while dropping the ssh gate");
                Vec::new()
            }
        }
    }

    /// Forgets every held key and the overflow state.
    pub(super) fn ssh_hold_clear(&mut self) {
        self.ssh_typeahead.clear();
        self.ssh_hold_drop_gate();
        self.ssh_iq.items = 0;
        self.ssh_iq.bytes = 0;
        self.ssh_iq.overflowed = false;
    }

    /// Takes everything held, in typed order, without delivering it.
    pub(super) fn ssh_hold_take(&mut self) -> Vec<HeldInput> {
        let mut held = self.ssh_hold_drop_gate();
        self.ssh_iq.items = 0;
        self.ssh_iq.bytes = 0;
        held.extend(self.ssh_typeahead.take());
        // Adjacent printable runs are one input, as the legacy buffer kept them.
        let mut merged = SshTypeahead::default();
        for input in held {
            merged.push(input);
        }
        merged.take()
    }

    /// The text the user would see had they typed the held keys into an empty editor, without
    /// running anything (for a connect that ended without a remote shell). Consumes the hold.
    pub(super) fn ssh_hold_take_unexecuted_text(&mut self) -> String {
        let mut collector = SshTypeahead::default();
        for input in self.ssh_hold_take() {
            collector.push(input);
        }
        collector.unexecuted_text()
    }

    /// Number of inputs held (for the log line, never content).
    pub(super) fn ssh_held_count(&self) -> usize {
        self.ssh_iq.items + usize::from(!self.ssh_typeahead.is_empty())
    }

    /// The hand-over: closes the gate, which is the delivery. The queue's one drain writes the
    /// held inputs in typed order, each routed from the facts of its own delivery.
    pub(super) fn ssh_hold_deliver(&mut self, ctx: &mut ViewContext<Self>) {
        let Some((_, key)) = self.ssh_iq.gate.take() else {
            // Legacy buffer (kill switch, or keys held while the queue was delivering).
            for input in self.ssh_typeahead.take() {
                self.deliver_held_input(input, ctx);
            }
            self.ssh_typeahead.clear();
            return;
        };
        trace::release(
            trace::legacy::SSH,
            self.ssh_iq.items,
            self.ssh_iq.bytes,
            "remote",
        );
        self.ssh_iq.items = 0;
        self.ssh_iq.bytes = 0;
        let queue = Rc::clone(&self.paste_queue.queue);
        let Ok(mut guard) = queue.try_borrow_mut() else {
            log::error!("ssh hold: input queue busy at the hand-over; held keys stay queued");
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

    /// Whether the open queue gates are only ssh ones: bytes that reach the pty writer then are
    /// not held behind them (the held keys were taken before they got there; anything else is
    /// passive traffic the legacy hold never held either).
    pub(super) fn only_ssh_gates_open(&self) -> bool {
        self.paste_queue.queue.try_borrow().is_ok_and(|queue| {
            !queue.gates().is_empty()
                && queue
                    .gates()
                    .iter()
                    .all(|gate| gate.kind == GateKind::SshConnect)
        })
    }
}
