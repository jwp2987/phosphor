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

use std::rc::Rc;
use std::time::Duration;

use warpui::ViewContext;
use warpui::r#async::Timer;

use super::{Input, PasteSource, clipboard_content_with_escaped_paths};
use crate::terminal::input_queue::{
    DeliverOutcome, DestFacts, Destination, DiscardReason, EntryKind, FollowUps, GateKind,
    GateSpec, GateToken, Host, InputEntry,
};
use crate::view_components::DismissibleToast;
use crate::workspace::ToastStack;

const PASTE_FAILED_TOAST: &str =
    "Paste failed \u{2014} the clipboard did not answer; your typing was kept";

/// The [`Host`] of the editor's paste queue, for the events that arrive at the input (the
/// reader's result, the give-up cap).
struct InputQueueHost<'a, 'b> {
    input: &'a mut Input,
    ctx: &'a mut ViewContext<'b, Input>,
    /// Entries queued behind the paste that is about to be delivered.
    held_behind: usize,
}

impl Host for InputQueueHost<'_, '_> {
    fn facts(&self) -> DestFacts {
        DestFacts {
            editor_focused: true,
            input_box_visible: true,
            ..DestFacts::default()
        }
    }

    fn deliver(
        &mut self,
        entry: &InputEntry,
        _dest: Destination,
        _follow: &mut FollowUps,
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
                }
                if self.held_behind > 0 {
                    self.input.request_detection_for_pasted_text(self.ctx);
                }
            }
            EntryKind::EditorKey(payload) => {
                if let Some(key) = payload.take() {
                    self.input.editor.update(self.ctx, |editor, ctx| {
                        editor.apply_held_paste_key(key, ctx)
                    });
                }
            }
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
                if self.ctx.has_singleton_model::<ToastStack>() {
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
    /// The give-up cap of paste `token` fired. A no-op if the reader's result already closed it.
    fn on_paste_queue_give_up(&mut self, token: GateToken, ctx: &mut ViewContext<Self>) {
        let queue = Rc::clone(self.editor.as_ref(ctx).paste_hold().queue());
        let mut host = InputQueueHost {
            input: self,
            ctx,
            held_behind: 0,
        };
        queue.borrow_mut().give_up(&mut host, token);
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
        let queue = Rc::clone(hold.queue());
        {
            let mut host = InputQueueHost {
                input: self,
                ctx,
                held_behind: 0,
            };
            queue.borrow_mut().offer_gated(
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
        let queue = Rc::clone(self.editor.as_ref(ctx).paste_hold().queue());
        // The paste itself is one of the queued entries; the rest are the keys behind it.
        let held_behind = if queue
            .borrow()
            .has_gate(crate::terminal::input_queue::GateKind::PasteReading, id)
        {
            queue.borrow().queued_summary().0.saturating_sub(1)
        } else {
            0
        };
        let result = match read {
            Some(warpui::clipboard::BackgroundTextRead::Content(content)) => {
                self.paste_payloads.insert(id, (source, content));
                // `Some(empty)` is the "ready" marker; the content waits in `paste_payloads`.
                Ok(Vec::new())
            }
            Some(warpui::clipboard::BackgroundTextRead::TimedOut) | None => Err(()),
        };
        {
            let mut host = InputQueueHost {
                input: self,
                ctx,
                held_behind,
            };
            queue.borrow_mut().on_paste_read(&mut host, id, result);
        }
        self.paste_payloads.remove(&id);
    }
}
