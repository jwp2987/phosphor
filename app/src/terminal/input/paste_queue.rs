//! Editor-bound paste ordering on the input queue (#1353, design phase 1).
//!
//! The editor-bound clipboard read is started here, not by the terminal view, so the queue it
//! uses is the editor's own (`EditorPasteHold::queue`): the paste is an entry held by a
//! `PasteReading` gate, the keys typed behind it are `EditorKey` entries the editor offers itself
//! (see `EditorView::hold_key_for_paste_on_queue`), the reader's result opens the gate, ^C
//! cancels the paste. The one timer is the give-up cap for a reader that never answers
//! (`PHOSPHOR_PASTE_BACKSTOP_MS`), armed through [`Host::arm_give_up`], token-guarded, logged.
//!
//! Delivery calls only the leaf appliers (`process_paste_content`, `apply_held_paste_key`), never
//! the key pipeline that holds, and a paste typed behind a paste starts its own read from the
//! editor event (it queues behind the first one's gate), so nothing here re-enters the queue.

use std::time::Duration;

use warpui::r#async::Timer;
use warpui::{SingletonEntity as _, ViewContext};

use super::{Input, PasteSource, clipboard_content_with_escaped_paths};
use crate::editor::DisplayMapStep;
use crate::terminal::input_queue::{
    DeliverOutcome, DestFacts, Destination, DiscardReason, EntryKind, FollowUps, GateKind,
    GateSpec, GateToken, Host, InputEntry, PaneQueue, is_pty_entry,
};
use crate::view_components::DismissibleToast;
use crate::workspace::ToastStack;

const PASTE_FAILED_TOAST: &str =
    "Paste failed \u{2014} the clipboard did not answer; your typing was kept";

/// The [`Host`] of the editor's paste queue, for the events that arrive at the input (the
/// reader's result, the give-up cap).
pub(super) struct InputQueueHost<'a, 'b> {
    input: &'a mut Input,
    ctx: &'a mut ViewContext<'b, Input>,
    /// Entries queued behind the paste that is about to be delivered.
    held_behind: usize,
}

impl<'a, 'b> InputQueueHost<'a, 'b> {
    pub(super) fn new(input: &'a mut Input, ctx: &'a mut ViewContext<'b, Input>) -> Self {
        Self {
            input,
            ctx,
            held_behind: 0,
        }
    }
}

impl Host for InputQueueHost<'_, '_> {
    fn facts(&self) -> DestFacts {
        DestFacts {
            editor_focused: true,
            input_box_visible: true,
            ..DestFacts::default()
        }
    }

    /// The input reaches no pty: on a queue shared with the terminal view, a drain driven from
    /// here stops at the first pty entry and the view continues it (#1353 step 3). Text and
    /// Enters typed behind an Enter that waited are routed by the view too (to the editor or the
    /// pty, decided when they are delivered), so the drain stops at those as well.
    fn handles(&self, entry: &InputEntry, _dest: Destination) -> bool {
        !is_pty_entry(&entry.kind) && !matches!(entry.kind, EntryKind::Text(_) | EntryKind::Enter)
    }

    fn deliver(
        &mut self,
        entry: &InputEntry,
        _dest: Destination,
        follow: &mut FollowUps,
    ) -> DeliverOutcome {
        match &entry.kind {
            EntryKind::Paste {
                paste_id,
                payload: Some(_),
            } => {
                let Some((source, content)) = self.input.paste_payloads.remove(paste_id) else {
                    log::warn!("input queue: ready paste {paste_id} had no content");
                    return DeliverOutcome::Delivered;
                };
                match source {
                    PasteSource::Clipboard => self.input.process_paste_content(content, self.ctx),
                    PasteSource::EditorMiddleClick => {
                        self.input.editor.update(self.ctx, |editor, ctx| {
                            editor.insert_middle_click_content(content, ctx)
                        });
                    }
                    PasteSource::TerminalMiddleClick(shell_dialect) => {
                        let text =
                            clipboard_content_with_escaped_paths(content, shell_dialect, false);
                        self.input.system_insert(&text, self.ctx);
                    }
                    PasteSource::TerminalPaste(shell_dialect) => {
                        let text =
                            clipboard_content_with_escaped_paths(content, shell_dialect, false);
                        self.input.paste_insert(&text, self.ctx);
                    }
                    PasteSource::PlainTextPaste => {
                        self.input
                            .paste_insert(content.plain_text.as_str(), self.ctx);
                    }
                }
                if self.held_behind > 0 {
                    self.input.request_detection_for_pasted_text(self.ctx);
                }
            }
            EntryKind::EditorKey(payload) => {
                // #1353 merge step 4: a key that reads the display map waits for it behind a
                // queue gate of its own (shared queue); the replay step closes it.
                let step = self
                    .input
                    .editor
                    .as_ref(self.ctx)
                    .paste_hold()
                    .display_map_step(payload, follow.flushing());
                if let DisplayMapStep::Wait(spec) = step {
                    let hold = self.input.editor.as_ref(self.ctx).paste_hold();
                    hold.request_display_step();
                    // A key that reads the soft-wrap layout also waits for a layout NEWER than the
                    // edits before it: the display map catching up is not enough for the last
                    // frame's layout (a large paste, #1389). Before the first frame there is none.
                    let soft = self.input.editor.as_ref(self.ctx).soft_wrap_state(self.ctx);
                    hold.set_layout_wait(
                        (payload.reads_soft_wrap_layout() && soft.is_laid_out())
                            .then(|| soft.generation()),
                    );
                    return DeliverOutcome::Requeue(spec);
                }
                if let Some(key) = payload.take() {
                    self.input
                        .editor
                        .update(self.ctx, |editor, ctx| editor.apply_queued_key(key, ctx));
                }
            }
            // Typed behind an Enter that waited (#1353 phase 3): the terminal view routes them
            // (editor or pty) when its drain delivers them; this host never does (`handles`).
            // A ^C that met the Enter's gate: the keys behind it were discarded and the ^C acts
            // through its own path.
            EntryKind::Signal(_) => {}
            other => log::warn!(
                "input queue: the input was handed an entry it does not deliver: {other:?}"
            ),
        }
        DeliverOutcome::Delivered
    }

    fn discarded(&mut self, entry: &InputEntry, why: DiscardReason) {
        if let EntryKind::Paste { paste_id, .. } = &entry.kind {
            self.input.paste_payloads.remove(paste_id);
        }
        match why {
            DiscardReason::PasteFailed | DiscardReason::PasteNotReady => {
                let refused = matches!(&entry.kind, EntryKind::Paste { paste_id, .. }
                    if self.input.paste_refused.contains(paste_id));
                if !refused && self.ctx.has_singleton_model::<ToastStack>() {
                    let window_id = self.ctx.window_id();
                    ToastStack::handle(self.ctx).update(self.ctx, |ts, ctx| {
                        ts.add_ephemeral_toast(
                            DismissibleToast::error(PASTE_FAILED_TOAST.to_string()),
                            window_id,
                            ctx,
                        );
                    });
                }
            }
            DiscardReason::PasteCancelled => {}
            // ^C met the Enter's gate: the keys typed behind that Enter are dropped with it.
            DiscardReason::SignalDiscard(GateKind::EnterVerdict | GateKind::TabCompletion) => {}
            other => log::warn!("input queue: editor paste entry discarded: {other:?}"),
        }
    }

    fn arm_give_up(&mut self, token: GateToken, _kind: GateKind, after: Duration) {
        // The ONE timer of a paste gate: a safety net for a reader that never answers. The reader's
        // result closes the gate first in every normal case and this then finds nothing to do.
        self.ctx.spawn(Timer::after(after), move |me, _, ctx| {
            me.on_paste_queue_give_up(token, ctx)
        });
    }

    fn warn(&mut self, line: &str) {
        log::warn!("{line}");
    }

    fn log_enabled(&self) -> bool {
        warpui::input_order::enabled()
    }

    fn log(&mut self, line: &str) {
        warpui::input_order::log_key_hop("kh_iq_queue", || line.to_owned());
    }
}

impl Input {
    /// Makes the terminal view's queue this input's queue (#1353 step 3): the editor adopts it, so
    /// the pane has ONE queue. False when the editor's own queue is busy (it then keeps it).
    pub(crate) fn adopt_pane_queue(
        &mut self,
        queue: PaneQueue,
        ctx: &mut ViewContext<Self>,
    ) -> bool {
        let adopted = self
            .editor
            .update(ctx, |editor, _| editor.paste_hold().adopt_queue(queue));
        if adopted {
            // #1353 merge step 4: held keys that read the display map wait behind a gate of the
            // shared queue; this input runs the replay step that closes it, once the effects of
            // the edits before the key have flushed (a stream item is delivered after them).
            let (tx, rx) = async_channel::bounded::<()>(1);
            self.editor.as_ref(ctx).paste_hold().enable_display_gate(tx);
            let _ = ctx.spawn_stream_local(
                rx,
                |me, _, ctx| me.on_display_map_step(ctx),
                |_, _| {}, /* on_done */
            );
        }
        adopted
    }

    /// The replay step of the editor's display-map gate (#1353 merge step 4, #1389): the effects of
    /// the edits applied before the waiting key have flushed, so the display map is current for
    /// it. Closes the gate; the queue's drain applies the key and what is behind it, up to the next
    /// key that needs a fresher map (which opens a new gate and requests another step).
    fn on_display_map_step(&mut self, ctx: &mut ViewContext<Self>) {
        let hold = self.editor.as_ref(ctx).paste_hold();
        // None: a ^C already flushed the gate; the late step finds nothing to do.
        let Some(key) = hold.take_display_gate() else {
            return;
        };
        if let Some(seen) = hold.layout_wait() {
            let soft = self.editor.as_ref(ctx).soft_wrap_state(ctx);
            if soft.generation() == seen {
                // No frame has been laid out since the edits: the gate stays closed and the
                // layout that lands runs this step again (the gate's cap is the logged give-up
                // for a view that is never rendered).
                hold.restore_display_gate(key);
                hold.arm_layout_wake(&soft);
                return;
            }
        }
        hold.set_layout_wait(None);
        let queue = hold.queue();
        let Some(mut guard) = queue.try_lock() else {
            log::warn!("input queue: display-map replay step found the queue busy");
            return;
        };
        hold.set_map_fresh(true);
        let mut host = InputQueueHost::new(self, ctx);
        guard.on_gate_event(&mut host, GateKind::EditorDisplayMap, key);
        hold.set_map_fresh(false);
    }

    /// An entry of the pane's shared queue that the editor side delivers, handed over by the
    /// terminal view's drain (#1353 step 3): the view reaches both destinations, so when its drain
    /// meets an editor key, an editor paste or text typed behind an Enter it delivers it through
    /// this input's own host, exactly as an editor-driven drain would have.
    pub(crate) fn deliver_pane_queue_entry(
        &mut self,
        entry: &InputEntry,
        follow: &mut FollowUps,
        ctx: &mut ViewContext<Self>,
    ) -> DeliverOutcome {
        let mut host = InputQueueHost::new(self, ctx);
        host.deliver(entry, Destination::Editor, follow)
    }

    /// The editor-side form of [`Host::discarded`] for an entry the view's drain removed.
    pub(crate) fn discard_pane_queue_entry(
        &mut self,
        entry: &InputEntry,
        why: DiscardReason,
        ctx: &mut ViewContext<Self>,
    ) {
        let mut host = InputQueueHost::new(self, ctx);
        host.discarded(entry, why);
    }

    /// The give-up cap of paste `token` fired. A no-op if the reader's result already closed it.
    fn on_paste_queue_give_up(&mut self, token: GateToken, ctx: &mut ViewContext<Self>) {
        let queue = self.editor.as_ref(ctx).paste_hold().queue();
        // #1353 phase 6 (INP-A4): a paste that gives up on its clipboard read signals the reader's
        // cancel flag, so the blocked-reader count and the log reflect an abandoned read.
        #[cfg(not(target_family = "wasm"))]
        {
            let is_paste_gate = queue.try_peek().is_some_and(|queue| {
                queue.gates().iter().any(|gate| {
                    gate.token == token
                        && gate.kind == crate::terminal::input_queue::GateKind::PasteReading
                })
            });
            if is_paste_gate {
                warpui::clipboard::abandon_pending_reads();
            }
        }
        let mut host = InputQueueHost {
            input: self,
            ctx,
            held_behind: 0,
        };
        queue.lock().give_up(&mut host, token);
    }

    /// Offers a paste whose text is already read (a platform that reads inline, #1405) to the
    /// pane queue as a ready entry. It is inserted at once when nothing is held, and otherwise
    /// takes its place behind what is: a clipboard read still pending, an Enter that waits, so
    /// it never lands ahead of text typed before it (#1437).
    pub(crate) fn offer_ready_paste(
        &mut self,
        content: warpui::clipboard::ClipboardContent,
        source: PasteSource,
        ctx: &mut ViewContext<Self>,
    ) {
        let hold = self.editor.as_ref(ctx).paste_hold();
        let id = hold.next_queue_paste_id();
        self.paste_payloads.insert(id, (source, content));
        let queue = hold.queue();
        let mut host = InputQueueHost {
            input: self,
            ctx,
            held_behind: 0,
        };
        queue.lock().offer(
            &mut host,
            EntryKind::Paste {
                paste_id: id,
                payload: Some(Vec::new()),
            },
        );
    }

    /// Starts an editor-bound clipboard read on the queue.
    #[cfg(not(target_family = "wasm"))]
    pub(super) fn start_queued_read(
        &mut self,
        reader: warpui::clipboard::BackgroundTextReader,
        backstop: Option<Duration>,
        source: PasteSource,
        ctx: &mut ViewContext<Self>,
    ) {
        let hold = self.editor.as_ref(ctx).paste_hold();
        let id = hold.next_queue_paste_id();
        let (tx, rx) = futures::channel::oneshot::channel();
        let spawned = std::thread::Builder::new()
            .name("paste-read".into())
            .spawn(move || {
                let _ = tx.send(reader());
            });
        if spawned.is_err() {
            self.show_paste_failed_toast(ctx);
            return;
        }
        ctx.spawn(async move { rx.await.ok() }, move |me, read, ctx| {
            me.finish_queued_read(id, read, source, ctx)
        });
        let queue = hold.queue();
        {
            let mut host = InputQueueHost {
                input: self,
                ctx,
                held_behind: 0,
            };
            queue.lock().offer_gated(
                &mut host,
                EntryKind::Paste {
                    paste_id: id,
                    payload: None,
                },
                GateSpec::new(GateKind::PasteReading, id).with_cap(backstop),
            );
        }
        // Say what is happening when the read takes noticeable time. Time is the requirement
        // here (user feedback), not correctness.
        ctx.spawn(
            Timer::after(Duration::from_millis(750)),
            move |me, _, ctx| {
                if me.editor.as_ref(ctx).paste_hold().is_pending(id)
                    && ctx.has_singleton_model::<ToastStack>()
                {
                    let window_id = ctx.window_id();
                    ToastStack::handle(ctx).update(ctx, |ts, ctx| {
                        ts.add_ephemeral_toast(
                            DismissibleToast::success(
                                "Pasting from the clipboard\u{2026} (Ctrl-C cancels)".to_string(),
                            ),
                            window_id,
                            ctx,
                        );
                    });
                }
            },
        );
    }

    #[cfg(not(target_family = "wasm"))]
    fn show_paste_failed_toast(&mut self, ctx: &mut ViewContext<Self>) {
        if ctx.has_singleton_model::<ToastStack>() {
            let window_id = ctx.window_id();
            ToastStack::handle(ctx).update(ctx, |ts, ctx| {
                ts.add_ephemeral_toast(
                    DismissibleToast::error(PASTE_FAILED_TOAST.to_string()),
                    window_id,
                    ctx,
                );
            });
        }
    }

    /// The reader's result for paste `id` (`None`: the reader thread died). Opens the paste's
    /// gate: the paste is applied, then the keys typed behind it in order. A cancelled or
    /// given-up paste is no longer in the queue and the late result changes nothing.
    #[cfg(not(target_family = "wasm"))]
    fn finish_queued_read(
        &mut self,
        id: u64,
        read: Option<warpui::clipboard::BackgroundTextRead>,
        source: PasteSource,
        ctx: &mut ViewContext<Self>,
    ) {
        let queue = self.editor.as_ref(ctx).paste_hold().queue();
        // The paste itself is one of the queued entries; the rest are the keys behind it.
        let held_behind = if queue
            .peek()
            .has_gate(crate::terminal::input_queue::GateKind::PasteReading, id)
        {
            queue.peek().queued_summary().0.saturating_sub(1)
        } else {
            0
        };
        let result = match read {
            Some(warpui::clipboard::BackgroundTextRead::Content(content)) => {
                self.paste_payloads.insert(id, (source, content));
                // `Some(empty)` is the "ready" marker; the content waits in `paste_payloads`.
                Ok(Vec::new())
            }
            Some(warpui::clipboard::BackgroundTextRead::TooLarge(bytes)) => {
                // Refused with its own message; the gate still opens so the keys typed behind it
                // are delivered in order.
                self.paste_refused.insert(id);
                self.show_paste_too_large_toast(bytes, ctx);
                Err(())
            }
            Some(warpui::clipboard::BackgroundTextRead::TimedOut) | None => Err(()),
        };
        {
            let mut host = InputQueueHost {
                input: self,
                ctx,
                held_behind,
            };
            queue.lock().on_paste_read(&mut host, id, result);
        }
        self.paste_payloads.remove(&id);
        self.paste_refused.remove(&id);
    }

    #[cfg(not(target_family = "wasm"))]
    pub(super) fn show_paste_too_large_toast(&mut self, bytes: usize, ctx: &mut ViewContext<Self>) {
        if ctx.has_singleton_model::<ToastStack>() {
            let window_id = ctx.window_id();
            ToastStack::handle(ctx).update(ctx, |ts, ctx| {
                ts.add_ephemeral_toast(
                    DismissibleToast::error(
                        crate::terminal::paste_sequencer::paste_too_large_message(bytes),
                    ),
                    window_id,
                    ctx,
                );
            });
        }
    }
}
