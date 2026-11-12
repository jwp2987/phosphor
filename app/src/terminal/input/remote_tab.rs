//! The remote Tab completion gate (#2188): Tab then a fast Enter in a remote session runs the
//! COMPLETED line.
//!
//! In a remote (managed ssh) session the completer answers over the link: the helper, or a side
//! command on the session's connection, at least one round trip. A local Tab's answer is held for
//! by the Enter itself (`EnterHold::TabCompletion`, #1165), and so is the remote shell's in-band
//! answer (#1801). The remote completer was the one case never waited for ("never wait on remote
//! I/O"): at a 150 ms round trip an Enter typed right after the Tab ran the line as typed and
//! cancelled the completion, and text typed after the Tab dropped its answer. Plain ssh gets both
//! right, because the Tab and the keys behind it reach the remote shell in order.
//!
//! The fix is one gate on the pane's existing queue, [`GateKind::RemoteTabCompletion`], opened by
//! the Tab at the moment its completer job starts. The queue holds every editor key behind it, in
//! order, as it holds keys behind an Enter or the history recall (#2107). The named event that
//! closes it is the END OF THAT JOB: its answer, applied to the buffer first
//! (`Input::deliver_tab_job_results`), or the job being superseded or cancelled. The drain then
//! replays the held keys in order, on the completed line.
//!
//! What is applied mirrors the local rule exactly: with an Enter among the held keys, a single
//! match is inserted and anything else (several candidates, none, an error) leaves the line as
//! typed and opens no menu, so the Enter runs the visible line. With no Enter held the answer is
//! handled as any Tab answer is (a menu may open) and the held text is typed after it.
//!
//! The one timer is the gate's give-up net (`GateKind::default_cap`, sized for the link): the
//! completion is cancelled and the held keys replay in order on the line as typed, which is what
//! plain ssh runs when the remote shell completes nothing. No key is dropped or reordered, and
//! the give-up is logged.
//!
//! ^C meeting the gate voids the completion and discards the keys typed behind the Tab, as it
//! voids an Enter held for a local Tab completion (`editor/view/mod.rs`).
//!
//! `PHOSPHOR_REMOTE_TAB_ENTER_WAIT=0` never opens the gate: the behaviour before #2188 exactly.
//!
//! The log lines `remote_tab_wait: released|gave_up why=.. waited_ms=.. held=.. enter_held=..`
//! carry counts only (the lab case `lab_tab_remote_dir` notes them).

use instant::Instant;
use warpui::{AppContext, ViewContext};

use super::Input;
use super::paste_queue::InputQueueHost;
use crate::terminal::input_queue::{Deferred, GateKind, GateSpec, GateToken, switches};

/// The open remote Tab completion gate of an input.
#[derive(Clone, Copy, Debug)]
pub(super) struct RemoteTabGate {
    /// The key of the queue gate.
    key: u64,
    /// The completer job whose end closes the gate. A later Tab that supersedes the job takes the
    /// gate over, so the superseded job's end does not close it.
    job: u64,
    /// When the gate opened, for the log.
    since: Instant,
}

impl Input {
    /// A Tab started a completion the remote side answers: closes the queue on the remote Tab
    /// gate, so the keys typed before the answer wait for it. Returns the job id the completer
    /// job reports its end with ([`Self::release_remote_tab_gate`]), or `None` when no gate was
    /// opened (the kill switch, or the queue already holds input: an Enter that waits, a paste
    /// being read, a replay in progress; the Tab then behaves as before #2188).
    pub(super) fn open_remote_tab_gate(&mut self, ctx: &mut ViewContext<Self>) -> Option<u64> {
        if !switches::remote_tab_enter_wait() {
            return None;
        }
        self.remote_tab_seq = self.remote_tab_seq.wrapping_add(1);
        let job = self.remote_tab_seq;
        if let Some(gate) = self.remote_tab_gate.as_mut() {
            // A Tab superseded the job the gate waits for: the gate now waits for this one.
            gate.job = job;
            let key = gate.key;
            warpui::input_order::log_hop("remote_tab_gate", || {
                format!("adopt key={key} job={job}")
            });
            return Some(job);
        }
        let queue = self.editor.as_ref(ctx).paste_hold().queue();
        let idle = !queue.has_deferred() && queue.try_peek().is_some_and(|held| held.is_idle());
        if !idle {
            warpui::input_order::log_hop("remote_tab_gate", || "skip queue_busy".to_owned());
            return None;
        }
        let key = job;
        let spec = GateSpec::new(GateKind::RemoteTabCompletion, key);
        match queue.try_lock_ordered() {
            Some(mut guard) => {
                let mut host = InputQueueHost::new(self, ctx);
                guard.open_gate(&mut host, spec);
            }
            // Opened by the next operation, in order (the busy-queue mailbox).
            None => queue.defer(Deferred::OpenGate(spec)),
        }
        self.remote_tab_gate = Some(RemoteTabGate {
            key,
            job,
            since: Instant::now(),
        });
        warpui::input_order::log_hop("remote_tab_gate", || {
            format!(
                "open key={key} job={job} cap_ms={}",
                GateKind::RemoteTabCompletion
                    .default_cap()
                    .unwrap_or_default()
                    .as_millis()
            )
        });
        Some(job)
    }

    /// Whether the remote Tab gate is open and waits for completer job `job`.
    pub(super) fn remote_tab_gate_waits_for(&self, job: u64) -> bool {
        self.remote_tab_gate.is_some_and(|gate| gate.job == job)
    }

    /// How many keys wait behind the remote Tab gate, and whether an Enter is among them. Zero
    /// and false when no gate is open (or the queue is busy: nothing can be read from it).
    pub(super) fn remote_tab_gate_held(&self, ctx: &AppContext) -> (usize, bool) {
        let Some(gate) = self.remote_tab_gate else {
            return (0, false);
        };
        self.editor
            .as_ref(ctx)
            .paste_hold()
            .queue()
            .try_peek()
            .map_or((0, false), |held| {
                (
                    held.held_behind_gate(GateKind::RemoteTabCompletion, gate.key),
                    held.holds_enter_behind_gate(GateKind::RemoteTabCompletion, gate.key),
                )
            })
    }

    /// Completer job `job` ended (`why` says how: its answer was handled, or it was superseded
    /// or cancelled): closes the gate that waited for it, and the drain replays the held keys in
    /// order. A no-op when the gate is gone or waits for a later job.
    pub(super) fn release_remote_tab_gate(
        &mut self,
        job: u64,
        why: &'static str,
        ctx: &mut ViewContext<Self>,
    ) {
        let Some(gate) = self.remote_tab_gate.filter(|gate| gate.job == job) else {
            return;
        };
        let (held, enter_held) = self.remote_tab_gate_held(ctx);
        self.remote_tab_gate = None;
        // Counts only, never the line.
        log::info!(
            "remote_tab_wait: released why={why} waited_ms={} held={held} enter_held={}",
            gate.since.elapsed().as_millis(),
            u8::from(enter_held)
        );
        self.close_remote_tab_gate(gate.key, ctx);
    }

    /// ^C voided the line while the remote Tab gate was open: the queue's signal plan has
    /// discarded the keys typed behind the Tab and removed the gate; the completion is cancelled
    /// here, so a late answer lands nowhere. A no-op when no gate is open (the common case).
    pub(super) fn cancel_remote_tab_gate(
        &mut self,
        why: &'static str,
        ctx: &mut ViewContext<Self>,
    ) {
        let Some(gate) = self.remote_tab_gate.take() else {
            return;
        };
        log::info!(
            "remote_tab_wait: released why={why} waited_ms={} held=0 enter_held=0",
            gate.since.elapsed().as_millis()
        );
        self.tab_completion_request = None;
        self.cancel_pending_completions_for_deferred_enter();
        // The signal already removed the gate; this only closes one it did not reach.
        self.close_remote_tab_gate(gate.key, ctx);
    }

    /// Closes the queue gate `key` (its named event), or records the event for the next operation
    /// when the queue is busy.
    fn close_remote_tab_gate(&mut self, key: u64, ctx: &mut ViewContext<Self>) {
        let queue = self.editor.as_ref(ctx).paste_hold().queue();
        let Some(mut guard) = queue.try_lock_ordered() else {
            queue.defer(Deferred::GateEvent {
                kind: GateKind::RemoteTabCompletion,
                key,
            });
            queue.defer(Deferred::FlushProgrammaticEdits);
            return;
        };
        let mut host = InputQueueHost::new(self, ctx);
        guard.on_gate_event(&mut host, GateKind::RemoteTabCompletion, key);
        drop(guard);
        self.flush_deferred_programmatic_edits(ctx);
    }

    /// The give-up net of the remote Tab gate fired (`token`): the completion never answered. It
    /// is cancelled first (a late answer must not land on, or after, a line that already ran),
    /// then the queue replays the held keys in order on the line as typed: an Enter among them
    /// runs that line, as plain ssh runs it when the remote shell completes nothing. None is
    /// dropped, none reordered.
    pub(crate) fn give_up_remote_tab_gate(
        &mut self,
        token: GateToken,
        ctx: &mut ViewContext<Self>,
    ) {
        let queue = self.editor.as_ref(ctx).paste_hold().queue();
        let (held, enter_held) = self.remote_tab_gate_held(ctx);
        let Some(mut guard) = queue.try_lock() else {
            // Run again, whole, once the queue is free (the cap is one-shot, the handler is
            // token-guarded).
            queue.defer(Deferred::GiveUpCap(token));
            return;
        };
        let waited = self
            .remote_tab_gate
            .take()
            .map(|gate| gate.since.elapsed())
            .unwrap_or_default();
        log::warn!(
            "remote_tab_wait: gave_up why=net waited_ms={} held={held} enter_held={} cap_ms={}; \
             the remote completion did not answer: it is cancelled and the held keys run in typed \
             order on the line as typed (#2188)",
            waited.as_millis(),
            u8::from(enter_held),
            GateKind::RemoteTabCompletion
                .default_cap()
                .unwrap_or_default()
                .as_millis()
        );
        self.tab_completion_request = None;
        self.cancel_pending_completions_for_deferred_enter();
        let mut host = InputQueueHost::new(self, ctx);
        guard.give_up(&mut host, token);
        drop(guard);
        self.flush_deferred_programmatic_edits(ctx);
    }

    /// Whether `token` names an open remote Tab completion gate on this input's queue.
    pub(crate) fn is_remote_tab_token(&self, token: GateToken, ctx: &AppContext) -> bool {
        self.editor
            .as_ref(ctx)
            .paste_hold()
            .queue()
            .try_peek()
            .is_some_and(|held| {
                held.gates()
                    .iter()
                    .any(|gate| gate.token == token && gate.kind == GateKind::RemoteTabCompletion)
            })
    }

    /// Test-only: whether the remote Tab gate is open.
    #[cfg(test)]
    pub(crate) fn remote_tab_gate_open_for_test(&self) -> bool {
        self.remote_tab_gate.is_some()
    }
}
