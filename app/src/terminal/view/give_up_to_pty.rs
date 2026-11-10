//! A hold that gives up writes its keys to the pty (owner decision 2026-11-09, REVERSING #1661).
//!
//! #1661 (2026-11-05) discarded the keys typed after an Enter whenever that Enter's gate gave up,
//! with a toast that counted them. No plain terminal ever drops typed bytes, and on a busy
//! machine or over a VPN the nets fire on healthy sessions (`docs/design/user-annoyance-review.md`
//! item 1). The rule now, for every give-up of a hold for the shell:
//!
//! * the held keys are written to the pty, the shell or whatever program reads the tty, in typed
//!   order: exactly the bytes a plain terminal would have sent all along;
//! * nothing is dropped, and no toast is shown for them (one log hop per give-up,
//!   `give_up_keys_to_pty`, counts only);
//! * only the visible line typed BEFORE the Enter stays in the editor, as before, when that
//!   Enter did not run.
//!
//! The families that end here: an Input-owned Enter gate (the route-settle net, the verdict and
//! Tab-completion caps, the unready-shell 60 s net, the gate's leak net, an edit that voided the
//! Enter; `Event::GivenUpKeysToPty` or the deferred `VoidEnterGate`), the deferred Enter's
//! `HandoffReport` gate (the report patience, the uncovered-report patience, the young verdict
//! cap, the gate's leak net; `give_up_deferred_enter_keys`), the paced CLI-agent submit's cap
//! and a young hold's gate the hand-off no longer knows ([`TerminalView::give_up_gate_keys_to_pty`]).
//! A young Enter's own cap writes through the hand-off (`release_young_hold_keys`) and the ssh
//! connect give-ups through `SshDropThen::GiveUpToPty`.
//!
//! Not changed (they are not give-ups, or they do not drop): ^C / ^D void typeahead as the tty
//! does; the multi-line dialog's answer discards what was typed over it (#1652); the subshell /
//! PowerShell setup nets and the ConPTY prompt hold (#1584, #1923) keep their own rules.
//!
//! Edge cases:
//! * a secret read (canonical, echo off) is up: the keys still go to the pty, to whatever reads
//!   it, as in a plain terminal; they are never shown as editor text;
//! * the pty is gone (the shell exited, the spawn failed): the keys become text in this pane's
//!   input, never dropped silently and never written anywhere else;
//! * the agent owns the running command and the write is refused: the same, text in the input;
//! * several held Enters keep their order (one write on a pty, one write per key on ConPTY,
//!   #1189);
//! * the write is made by the pane that held the keys, into its own pty, at the give-up: a
//!   give-up during an ssh connect therefore reaches the ssh process, not the local shell that
//!   follows a later exit;
//! * an earlier gate still holds the queue: the keys stay queued behind it, in typed order
//!   (`InputQueue::close_gate_for_give_up`), and are delivered when it opens.
//!
//! The write goes through `write_user_bytes_to_pty_after_paste_stage`, the stages every typed
//! key takes after the queue, so the typeahead hand-off counts the bytes like any others (a tail
//! with no Enter comes back into the editor with the shell's next line report). It is a
//! take-and-write, not a table-routed delivery: both `PHOSPHOR_QUEUE_DESTINATION` modes run the
//! same code.

use std::borrow::Cow;

use warpui::ViewContext;

use super::paste_queue::PtyQueueHost;
use super::{Event, TerminalView};
use crate::editor::{EditorAction, HeldPasteKey};
use crate::terminal::input::count_discarded_keys;
use crate::terminal::input_queue::{
    Deferred, EntryKind, GateKind, GaveUpKeys, HELD_PASTE, InputEntry,
};
use crate::terminal::model::typeahead_handoff::text_for_editor;

impl TerminalView {
    /// Whether a paste written by a give-up is bracketed: what a paste made now would be, read
    /// once per give-up.
    fn given_up_paste_is_bracketed(
        &self,
        cached: &mut Option<bool>,
        ctx: &mut ViewContext<Self>,
    ) -> bool {
        if let Some(bracketed) = *cached {
            return bracketed;
        }
        let bracketed = self.paste_routing(self.agent_owns_command_pty(), ctx).1;
        *cached = Some(bracketed);
        bracketed
    }

    /// The bytes a plain terminal would have sent for `entries` (taken out of a gate that gave
    /// up), one chunk per entry, in typed order.
    ///
    /// Text and Enters typed in the editor behind an Enter are their characters and `\r`; an
    /// editor key that is plain typing (text, Enter, Backspace) is its byte; a paste is its
    /// content (bracketed when the terminal asked for it); pty keys are the bytes they already
    /// are. An editor key with no byte form here (a cursor motion, a selection) acts on the
    /// editor, as it does when a deferred Enter's keys are released (`pty_bytes_of_taken_entries`):
    /// it is not dropped. A signal entry left in the segment has already acted through its own
    /// path and is not written a second time.
    pub(super) fn given_up_entry_chunks(
        &mut self,
        entries: Vec<InputEntry>,
        ctx: &mut ViewContext<Self>,
    ) -> Vec<Vec<u8>> {
        let mut bracketed: Option<bool> = None;
        let mut chunks = Vec::with_capacity(entries.len());
        for entry in entries {
            match entry.kind {
                EntryKind::Text(text) => chunks.push(text.into_bytes()),
                EntryKind::Enter => chunks.push(b"\r".to_vec()),
                EntryKind::Control(text) | EntryKind::Seq(text) => {
                    chunks.push(text.as_bytes().to_vec())
                }
                EntryKind::PtyBytes { bytes, .. } | EntryKind::Raw(bytes) => chunks.push(bytes),
                EntryKind::Signal(_) => {}
                EntryKind::Held { class, text } => {
                    if class == HELD_PASTE {
                        let bracketed = self.given_up_paste_is_bracketed(&mut bracketed, ctx);
                        chunks.push(super::paste_bytes_for_pty(&text, bracketed));
                    } else {
                        chunks.push(text.into_bytes());
                    }
                }
                EntryKind::EditorKey(payload) => match payload.take() {
                    Some(HeldPasteKey::Text(text)) => chunks.push(text.into_bytes()),
                    Some(HeldPasteKey::Action(EditorAction::Enter)) => chunks.push(b"\r".to_vec()),
                    Some(HeldPasteKey::Action(EditorAction::Backspace)) => chunks.push(vec![0x7f]),
                    Some(key) => {
                        self.input.update(ctx, |input, ctx| {
                            input
                                .editor()
                                .update(ctx, |editor, ctx| editor.apply_queued_key(key, ctx))
                        });
                    }
                    None => {}
                },
                EntryKind::Paste { paste_id, payload } => {
                    let routed = self.paste_queue.routed_pastes.remove(&paste_id);
                    self.paste_queue.ssh_pastes.remove(&paste_id);
                    self.paste_queue.refused.remove(&paste_id);
                    if let Some(bytes) = self.paste_queue.payloads.remove(&paste_id) {
                        if routed {
                            // Read as TEXT for a hold that routes it at delivery (ssh, a CLI
                            // agent): a paste for the pty now.
                            let bracketed = self.given_up_paste_is_bracketed(&mut bracketed, ctx);
                            chunks.push(super::paste_bytes_for_pty(
                                &String::from_utf8_lossy(&bytes),
                                bracketed,
                            ));
                        } else {
                            // Already prepared as pty bytes when it was read.
                            chunks.push(bytes);
                        }
                    } else if let Some(text) = self
                        .input
                        .update(ctx, |input, _| input.take_paste_payload_text(paste_id))
                    {
                        // An editor paste keeps its content in the input.
                        let bracketed = self.given_up_paste_is_bracketed(&mut bracketed, ctx);
                        chunks.push(super::paste_bytes_for_pty(&text, bracketed));
                    } else if let Some(bytes) = payload.filter(|bytes| !bytes.is_empty()) {
                        chunks.push(bytes);
                    } else {
                        // A paste whose clipboard read never finished keeps its own gate
                        // (`close_gate_for_give_up`), so this is a paste nobody has content for.
                        log::warn!(
                            "input queue: a paste taken by a give-up had no content (paste \
                             {paste_id}); nothing to write for it"
                        );
                    }
                }
            }
        }
        chunks
    }

    /// Owner decision 2026-11-09 (reversing #1661): writes `entries`, the keys a hold held when
    /// it gave up (`why` names the give-up), to this pane's pty in typed order. Returns how many
    /// keys they were. See the module note for the rule and its edge cases.
    pub(super) fn write_given_up_entries_to_pty(
        &mut self,
        entries: Vec<InputEntry>,
        why: &'static str,
        ctx: &mut ViewContext<Self>,
    ) -> usize {
        let keys = count_discarded_keys(&entries);
        if keys == 0 {
            return 0;
        }
        let chunks = self.given_up_entry_chunks(entries, ctx);
        self.write_given_up_chunks_to_pty(chunks, keys, why, ctx);
        keys
    }

    /// [`Self::write_given_up_entries_to_pty`] for keys already turned into their bytes, one
    /// chunk per key, in typed order.
    pub(super) fn write_given_up_chunks_to_pty(
        &mut self,
        chunks: Vec<Vec<u8>>,
        keys: usize,
        why: &'static str,
        ctx: &mut ViewContext<Self>,
    ) {
        let chunks: Vec<Vec<u8>> = chunks
            .into_iter()
            .filter(|chunk| !chunk.is_empty())
            .collect();
        let bytes: usize = chunks.iter().map(Vec::len).sum();
        let pty_gone = self.pty_gone;
        let mut written = 0usize;
        // What could not be written: the pty is gone, or the agent owns the running command.
        let mut for_editor: Vec<u8> = Vec::new();
        if pty_gone {
            for_editor = chunks.concat();
        } else {
            // One write per key on ConPTY (#1189), one write elsewhere.
            let writes = self.handoff.read().held_key_release_writes(chunks);
            let mut writes = writes.into_iter();
            // The hand-off counts these bytes but starts no young Enter's hold for them
            // (`TypeaheadHandoff::on_given_up_user_bytes`): a give-up must not arm another net.
            self.writing_given_up_keys = true;
            for chunk in writes.by_ref() {
                let len = chunk.len();
                if self.write_user_bytes_to_pty_after_paste_stage(
                    Cow::Owned(chunk.clone()),
                    true,
                    ctx,
                ) {
                    written += len;
                } else {
                    for_editor = chunk;
                    break;
                }
            }
            self.writing_given_up_keys = false;
            for chunk in writes {
                for_editor.extend_from_slice(&chunk);
            }
        }
        let to_editor = for_editor.len();
        if to_editor > 0 {
            let text = text_for_editor(&for_editor);
            if !text.is_empty() {
                // Never dropped silently: visible text in this pane's input, not run, and the
                // first Enter on it is held with the "check the line" toast.
                self.input.update(ctx, |input, ctx| {
                    input.system_insert(&text, ctx);
                    let buffer = input.editor().as_ref(ctx).buffer_text(ctx);
                    input.arm_handed_back_enter_guard(buffer);
                });
            }
        }
        if bytes > 0 {
            log::warn!(
                "input: a hold gave up ({why}); {keys} held key(s), {bytes} byte(s): {written} \
                 byte(s) written to the pty in typed order, {to_editor} byte(s) put in the input \
                 as text (pty_gone={pty_gone}); none dropped"
            );
        }
        warpui::input_order::log_hop("give_up_keys_to_pty", || {
            format!(
                "why={why} keys={keys} bytes={bytes} written={written} to_editor={to_editor} \
                 pty_gone={}",
                u8::from(pty_gone)
            )
        });
        self.redetermine_terminal_focus(ctx);
        if written > 0 {
            // A tail with no Enter is on the shell's line now: the hand-off decides again, so the
            // shell's report brings it into the editor like any typeahead.
            ctx.emit(Event::RetryTypeaheadHandoffRequest);
        }
    }

    /// A timer gave up on gate `kind`/`key` of the pane queue (a paced CLI-agent submit's cap, a
    /// young hold's gate the hand-off no longer knows, an Enter gate the input no longer tracks).
    /// The gate is closed and what it held is written to the pty in typed order (owner decision
    /// 2026-11-09, reversing #1661). With the queue busy this is one deferred operation, applied
    /// in FIFO order once it is free, so a key typed after the give-up follows what the gate held.
    pub(super) fn give_up_gate_keys_to_pty(
        &mut self,
        kind: GateKind,
        key: u64,
        why: &'static str,
        ctx: &mut ViewContext<Self>,
    ) {
        let queue = self.paste_queue.queue.clone();
        let gave_up = {
            let Some(mut guard) = queue.try_lock_ordered() else {
                queue.defer(Deferred::GiveUpGateKeysToPty { kind, key, why });
                return;
            };
            let mut host = PtyQueueHost::new(self, ctx);
            guard.close_gate_for_give_up(&mut host, kind, key)
        };
        self.finish_give_up_gate_keys_to_pty(gave_up, kind, key, why, ctx);
    }

    /// The tail of [`Self::give_up_gate_keys_to_pty`], with the gate's entries taken (at once, or
    /// by the deferred `GiveUpGateKeysToPty`).
    pub(super) fn finish_give_up_gate_keys_to_pty(
        &mut self,
        gave_up: GaveUpKeys,
        kind: GateKind,
        key: u64,
        why: &'static str,
        ctx: &mut ViewContext<Self>,
    ) {
        if gave_up.left_in_place > 0 {
            // Behind an earlier gate: they stay queued in typed order and follow when it opens.
            warpui::input_order::log_hop("give_up_keys_left_in_place", || {
                format!(
                    "why={why} gate={key} kind={kind:?} entries={}",
                    gave_up.left_in_place
                )
            });
            return;
        }
        let keys = self.write_given_up_entries_to_pty(gave_up.entries, why, ctx);
        if keys == 0 {
            warpui::input_order::log_hop("give_up_keys_to_pty", || {
                format!("why={why} keys=0 bytes=0 written=0 to_editor=0 pty_gone=0")
            });
        }
    }
}
