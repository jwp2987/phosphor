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
//! * a later line never runs without the line typed before it (below).
//!
//! KEEP THEM TOGETHER (owner decision 2026-11-09, second ruling). The first ruling left the line
//! typed BEFORE an Enter that did not run in the editor and wrote the keys typed after it to the
//! pty: `cd build`, Enter, `make clean`, Enter then ran `make clean` alone, in the wrong
//! directory. The keys now go where the line before them went:
//!
//! * the line RAN (a submitted Enter whose route never settled, a young Enter's hold, a paced
//!   CLI-agent submit, an ssh connect): the keys are written to the pty in typed order
//!   ([`TerminalView::write_given_up_entries_to_pty`]);
//! * the line did NOT run and stays in the editor because where it goes is undecided (its
//!   verdict or Tab completion never came, the shell's report never came, an edit voided the
//!   Enter): the keys stay WITH it in the editor, as visible text on the lines after it, in
//!   typed order, under the hand-back guard. Nothing reaches the pty
//!   ([`TerminalView::keep_given_up_keys_with_line`]);
//! * the line did not run but is certainly a shell line (an Enter held for an unready shell:
//!   only the shell's readiness was missing): the line, its Enter and the keys after it are all
//!   written to the pty in typed order, like a plain terminal; a bare Enter on an empty line is
//!   written too ([`TerminalView::write_unready_line_and_keys_to_pty`]);
//! * a gate nobody tracks any more (an orphan Enter gate, a report gate the hand-off forgot):
//!   whether its Enter ran is unknown, so with a line in the editor the keys stay with it, and
//!   with an empty editor (no unrun line to overtake) they go to the pty.
//!
//! The families that end here: an Input-owned Enter gate (the route-settle net, the verdict and
//! Tab-completion caps, the unready-shell 60 s net, the gate's leak net, an edit that voided the
//! Enter; `Event::GivenUpKeysToPty` / `GivenUpKeysWithLine` / `GivenUpLineAndKeysToPty`, or the
//! deferred `VoidEnterGate`), the deferred Enter's `HandoffReport` gate (the report patience,
//! the uncovered-report patience, the young verdict cap, the gate's leak net;
//! `give_up_deferred_enter_keys`), the paced CLI-agent submit's cap, a pty paste whose clipboard
//! read never answered and a young hold's gate the hand-off no longer knows
//! ([`TerminalView::give_up_gate_keys_to_pty`]). A young Enter's own cap writes through the
//! hand-off (`release_young_hold_keys`), and the ssh connect give-ups and the ssh
//! bootstrap-failure timer through `SshDropThen::GiveUpToPty`.
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
use crate::terminal::input::{KeysAfterEnter, count_discarded_keys};
use crate::terminal::input_queue::{
    Deferred, DiscardReason, EntryKind, GateKind, GaveUpKeys, HELD_PASTE, Host as _, InputEntry,
};
use crate::terminal::model::typeahead_handoff::text_for_editor;

/// What one entry taken out of a gate that gave up is, for the place its keys go.
enum GivenUpEntry {
    /// The bytes a plain terminal would have sent for it.
    Bytes(Vec<u8>),
    /// An editor key with no byte form (a cursor motion, a selection): it acts on the editor.
    EditorKey(HeldPasteKey),
    /// Nothing to place: a signal that already acted, a paste nobody has content for.
    Nothing,
}

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
            match self.given_up_entry(entry, &mut bracketed, ctx) {
                GivenUpEntry::Bytes(bytes) => chunks.push(bytes),
                GivenUpEntry::EditorKey(key) => self.apply_given_up_editor_key(key, ctx),
                GivenUpEntry::Nothing => {}
            }
        }
        chunks
    }

    /// An editor key with no byte form among given-up keys acts on the editor.
    fn apply_given_up_editor_key(&mut self, key: HeldPasteKey, ctx: &mut ViewContext<Self>) {
        self.input.update(ctx, |input, ctx| {
            input
                .editor()
                .update(ctx, |editor, ctx| editor.apply_queued_key(key, ctx))
        });
    }

    /// Whether `entry` is a paste nobody has content for: its clipboard read never answered (the
    /// give-up of its own `PasteReading` gate; a paste typed behind another gate keeps its gate,
    /// `close_gate_for_give_up`).
    fn is_unread_given_up_paste(&self, entry: &InputEntry, ctx: &mut ViewContext<Self>) -> bool {
        let EntryKind::Paste { paste_id, payload } = &entry.kind else {
            return false;
        };
        payload.as_ref().is_none_or(|bytes| bytes.is_empty())
            && !self.paste_queue.payloads.contains_key(paste_id)
            && self
                .input
                .as_ref(ctx)
                .peek_paste_payload_text(*paste_id)
                .is_none()
    }

    /// One entry of [`Self::given_up_entry_chunks`]. `bracketed` caches whether a paste is
    /// bracketed (`Some(false)` from a caller that wants plain text).
    fn given_up_entry(
        &mut self,
        entry: InputEntry,
        bracketed: &mut Option<bool>,
        ctx: &mut ViewContext<Self>,
    ) -> GivenUpEntry {
        if self.is_unread_given_up_paste(&entry, ctx) {
            // The paste itself is the one thing that cannot be placed: its content never came.
            // It is discarded with the paste's own notice and its records go (owner decision
            // 2026-11-09: what was typed behind it is written to the pty like the rest).
            let mut host = PtyQueueHost::new(self, ctx);
            host.discarded(&entry, DiscardReason::PasteNotReady);
            return GivenUpEntry::Nothing;
        }
        match entry.kind {
            EntryKind::Text(text) => GivenUpEntry::Bytes(text.into_bytes()),
            EntryKind::Enter => GivenUpEntry::Bytes(b"\r".to_vec()),
            EntryKind::Control(text) | EntryKind::Seq(text) => {
                GivenUpEntry::Bytes(text.as_bytes().to_vec())
            }
            EntryKind::PtyBytes { bytes, .. } | EntryKind::Raw(bytes) => GivenUpEntry::Bytes(bytes),
            EntryKind::Signal(_) => GivenUpEntry::Nothing,
            EntryKind::Held { class, text } => {
                if class == HELD_PASTE {
                    let bracketed = self.given_up_paste_is_bracketed(bracketed, ctx);
                    GivenUpEntry::Bytes(super::paste_bytes_for_pty(&text, bracketed))
                } else {
                    GivenUpEntry::Bytes(text.into_bytes())
                }
            }
            EntryKind::EditorKey(payload) => match payload.take() {
                Some(HeldPasteKey::Text(text)) => GivenUpEntry::Bytes(text.into_bytes()),
                Some(HeldPasteKey::Action(EditorAction::Enter)) => {
                    GivenUpEntry::Bytes(b"\r".to_vec())
                }
                Some(HeldPasteKey::Action(EditorAction::Backspace)) => {
                    GivenUpEntry::Bytes(vec![0x7f])
                }
                Some(key) => GivenUpEntry::EditorKey(key),
                None => GivenUpEntry::Nothing,
            },
            EntryKind::Paste { paste_id, payload } => {
                let routed = self.paste_queue.routed_pastes.remove(&paste_id);
                self.paste_queue.ssh_pastes.remove(&paste_id);
                self.paste_queue.refused.remove(&paste_id);
                if let Some(bytes) = self.paste_queue.payloads.remove(&paste_id) {
                    if routed {
                        // Read as TEXT for a hold that routes it at delivery (ssh, a CLI
                        // agent): a paste for the pty now.
                        let bracketed = self.given_up_paste_is_bracketed(bracketed, ctx);
                        GivenUpEntry::Bytes(super::paste_bytes_for_pty(
                            &String::from_utf8_lossy(&bytes),
                            bracketed,
                        ))
                    } else {
                        // Already prepared as pty bytes when it was read.
                        GivenUpEntry::Bytes(bytes)
                    }
                } else if let Some(text) = self
                    .input
                    .update(ctx, |input, _| input.take_paste_payload_text(paste_id))
                {
                    // An editor paste keeps its content in the input.
                    let bracketed = self.given_up_paste_is_bracketed(bracketed, ctx);
                    GivenUpEntry::Bytes(super::paste_bytes_for_pty(&text, bracketed))
                } else if let Some(bytes) = payload.filter(|bytes| !bytes.is_empty()) {
                    GivenUpEntry::Bytes(bytes)
                } else {
                    // Unread pastes were handled above; this one lost its content in between.
                    log::warn!(
                        "input queue: a paste taken by a give-up had no content (paste \
                         {paste_id}); nothing to write for it"
                    );
                    GivenUpEntry::Nothing
                }
            }
        }
    }

    /// Owner decision 2026-11-09 ("keep them together"): the gate of an Enter that did NOT run
    /// gave up (`why` names the give-up) and its line stays in the editor. `entries`, the keys
    /// typed after that Enter, stay WITH the line: they become visible text after it, in typed
    /// order, the Enter itself and every Enter among them a line break. Nothing is written to the
    /// pty (a later line never runs without the line before it), nothing is dropped and no toast
    /// counts keys; the hand-back guard holds the first Enter on the buffer with its "check the
    /// line" notice, so the lines never run as one command unseen. Returns how many keys they
    /// were.
    ///
    /// The text is what the shell's line editor would have left of the keys (`text_for_editor`:
    /// Backspace erases, a paste is its plain text). An editor key with no byte form (a cursor
    /// motion, a selection) acts on the editor in its place, after the text typed before it.
    pub(super) fn keep_given_up_keys_with_line(
        &mut self,
        entries: Vec<InputEntry>,
        why: &'static str,
        ctx: &mut ViewContext<Self>,
    ) -> usize {
        let keys = count_discarded_keys(&entries);
        if keys == 0 {
            return 0;
        }
        // The keys were typed after the Enter of the whole line: they follow its end, wherever
        // the cursor was when Enter was pressed.
        let line_chars = self.input.update(ctx, |input, ctx| {
            input
                .editor()
                .update(ctx, |editor, ctx| editor.move_to_buffer_end(ctx));
            input.editor().as_ref(ctx).buffer_text(ctx).chars().count()
        });
        let mut run: Vec<u8> = Vec::new();
        if line_chars > 0 {
            // The Enter that did not run: the keys start on the line after it.
            run.push(b'\r');
        }
        // Plain text for the editor: a paste is never wrapped in the bracketed-paste markers.
        let mut bracketed = Some(false);
        let mut chars = 0usize;
        for entry in entries {
            match self.given_up_entry(entry, &mut bracketed, ctx) {
                GivenUpEntry::Bytes(bytes) => run.extend_from_slice(&bytes),
                GivenUpEntry::EditorKey(key) => {
                    self.insert_kept_run(&mut run, &mut chars, ctx);
                    self.apply_given_up_editor_key(key, ctx);
                }
                GivenUpEntry::Nothing => {}
            }
        }
        self.insert_kept_run(&mut run, &mut chars, ctx);
        if chars > 0 {
            // Several lines in one buffer: the first Enter on it is held with the "check the
            // line" notice, never run unseen.
            self.input.update(ctx, |input, ctx| {
                let buffer = input.editor().as_ref(ctx).buffer_text(ctx);
                input.arm_handed_back_enter_guard(buffer);
            });
            self.note_keys_follow_editor();
        }
        log::warn!(
            "input: a hold gave up ({why}) on an Enter that did not run; its line stays in the \
             input and the {keys} key(s) typed after it stay with it as {chars} char(s) of text, \
             in typed order; nothing written to the pty, none dropped"
        );
        warpui::input_order::log_hop("give_up_keys_kept_with_line", || {
            format!("why={why} keys={keys} chars={chars} line_chars={line_chars}")
        });
        self.note_editor_took_over();
        self.redetermine_terminal_focus(ctx);
        keys
    }

    /// Puts the run of given-up keys collected so far into the editor as text, and empties it.
    fn insert_kept_run(
        &mut self,
        run: &mut Vec<u8>,
        chars: &mut usize,
        ctx: &mut ViewContext<Self>,
    ) {
        if run.is_empty() {
            return;
        }
        let text = text_for_editor(run);
        run.clear();
        if text.is_empty() {
            return;
        }
        *chars += text.chars().count();
        self.input
            .update(ctx, |input, ctx| input.system_insert(&text, ctx));
    }

    /// Owner decision 2026-11-09 ("keep them together"): the net of an Enter held for an unready
    /// shell gave up. Its line is certainly a shell line, so the line, its Enter and `entries`
    /// (the keys typed after it, possibly none) are written to the pty in typed order, like a
    /// plain terminal. A bare Enter on an empty line is written too (`\r`, then the keys). With
    /// the pty gone they all become text in the input, together (`write_given_up_chunks_to_pty`).
    pub(super) fn write_unready_line_and_keys_to_pty(
        &mut self,
        entries: Vec<InputEntry>,
        why: &'static str,
        ctx: &mut ViewContext<Self>,
    ) {
        let line = self
            .input
            .update(ctx, |input, ctx| input.take_given_up_unready_line(ctx));
        let mut keys = count_discarded_keys(&entries);
        let mut chunks: Vec<Vec<u8>> = Vec::with_capacity(entries.len() + 2);
        if let Some(line) = line {
            if line.contains('\n') {
                // Several lines (a paste, Shift-Enter): they go as the paste they would be.
                let bracketed = self.paste_routing(self.agent_owns_command_pty(), ctx).1;
                chunks.push(super::paste_bytes_for_pty(&line, bracketed));
            } else if !line.is_empty() {
                chunks.push(line.into_bytes());
            }
            chunks.push(b"\r".to_vec());
            keys += 1;
            // An Enter that found the held line's gate gone gave the line up and is not
            // absorbed (owner decision 2026-11-09): the text typed into the editor after the
            // line, then that Enter, follow the line and its Enter. Nothing is queued behind a
            // gate that is gone, so `entries` is empty then; these go first in any case (they
            // were in the editor with the line).
            let after = self.input.update(ctx, |input, _| {
                input.take_given_up_enter_after_unready_line()
            });
            if let Some(after) = after {
                if after.contains('\n') {
                    let bracketed = self.paste_routing(self.agent_owns_command_pty(), ctx).1;
                    chunks.push(super::paste_bytes_for_pty(&after, bracketed));
                    keys += 1;
                } else if !after.is_empty() {
                    chunks.push(after.into_bytes());
                    keys += 1;
                }
                chunks.push(b"\r".to_vec());
                keys += 1;
            }
        }
        chunks.extend(self.given_up_entry_chunks(entries, ctx));
        if keys == 0 {
            return;
        }
        self.write_given_up_chunks_to_pty(chunks, keys, why, ctx);
    }

    /// The entries `taken` out of a given-up Enter gate of the input go where `then` says
    /// ([`KeysAfterEnter`]): the one place for the free queue's events and the busy queue's
    /// deferred take, so both do the same.
    pub(super) fn place_given_up_enter_gate_keys(
        &mut self,
        taken: Vec<InputEntry>,
        then: KeysAfterEnter,
        ctx: &mut ViewContext<Self>,
    ) {
        match then {
            KeysAfterEnter::ToPty => {
                self.write_given_up_entries_to_pty(taken, "enter_gate", ctx);
            }
            KeysAfterEnter::WithLine => {
                self.keep_given_up_keys_with_line(taken, "enter_gate", ctx);
            }
            KeysAfterEnter::LineAndKeysToPty => {
                self.write_unready_line_and_keys_to_pty(taken, "unready_enter", ctx);
            }
        }
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
    /// pty paste's read cap, a young hold's gate the hand-off no longer knows, an Enter gate the
    /// input no longer tracks). The gate is closed and what it held is written to the pty in
    /// typed order (owner decision 2026-11-09, reversing #1661); for a gate nobody tracks any
    /// more the keys stay with a line the editor still holds instead ("keep them together",
    /// `finish_give_up_gate_keys_to_pty`). With the queue busy this is one deferred operation,
    /// applied in FIFO order once it is free, so a key typed after the give-up follows what the
    /// gate held.
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
        // A gate nobody tracks any more: whether its Enter ran is unknown. With a line in the
        // editor the keys stay with it (never run ahead of a line that may not have run); with an
        // empty editor there is no such line and they go to the pty.
        let untracked = matches!(why, "orphan_enter_gate" | "handoff_gate_leak_net");
        if untracked && !self.input.as_ref(ctx).buffer_text(ctx).is_empty() {
            self.keep_given_up_keys_with_line(gave_up.entries, why, ctx);
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
