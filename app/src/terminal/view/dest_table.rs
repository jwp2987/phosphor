//! The terminal view's side of the destination table (#1424).
//!
//! Step 3 (this file's first half): the SHADOW hooks. In `PHOSPHOR_QUEUE_DESTINATION=shadow` the
//! legacy entry points (`typed_characters_on_terminal`, `keydown_on_terminal`,
//! `control_sequence_on_terminal`, the replayed Enter) are wrapped: [`TerminalView::shadow_begin`]
//! builds the facts and asks the table what it would decide, BEFORE the legacy chain touches any
//! state; the legacy chain runs unchanged and notes the destination class it reached at its leaf
//! (`shadow::note_leaf`); [`TerminalView::shadow_end`] compares the two and logs a disagreement
//! (`kh_dest_disagree`). Nothing here changes where an input goes.

use warpui::ViewContext;

use super::TerminalView;
use crate::terminal::input_queue::shadow;
use crate::terminal::input_queue::table::{self, Class, Intent, Plan};
use crate::terminal::input_queue::{Origin, QueueFacts, RouteFacts};

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
