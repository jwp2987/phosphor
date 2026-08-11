//! [`StartAgentExecutor`]: shared child-agent dispatch/outcome bridge.
//!
//! Ported from the pin's `app/src/ai/blocklist/action_model/execute/start_agent.rs`
//! (`02b53fcd8`), LOCAL half only -- see `DECLINED.md`'s reversal of the
//! blanket orchestration decline (~:179) and its "Orchestration
//! config-picker layer" row (~:186), which keeps the cloud-runner half
//! declined under #290. `crates/warp_tui/src/orchestration_model.rs`'s
//! module doc explains why this frontend-neutral executor lives here (in the
//! `app` crate, not `warp_tui`) rather than being TUI-only: the pin's GUI
//! (`app/src/pane_group/pane/terminal_pane.rs`, `app/src/terminal/view.rs`)
//! and TUI both dispatch through the same `StartAgentExecutor` type, and the
//! GUI half of that wiring is future work here, not something this file
//! should preclude.
//!
//! ## What was cut, and why
//!
//! - **`Remote` execution and everything server-side that resolves a
//!   pending request as started** (`OrchestrationEventStreamer`,
//!   `register_watched_run_id`, `ConversationServerTokenAssigned`). All of
//!   it exists only to track a run hosted on Warp's servers; this fork has
//!   no remote runner (`is_remote_child` is permanently false).
//! - **The pin's `NewConversationRequestComplete`-based resolution
//!   mechanism.** The pin links a pending request to its child conversation
//!   by watching `BlocklistAIHistoryEvent::NewConversationRequestComplete`
//!   and `ConversationServerTokenAssigned` -- both fired only for
//!   server-created conversations, which this fork's
//!   [`crate::ai::blocklist::history_model::BlocklistAIHistoryModel`] does
//!   not have (no server, no assigned tokens). Rather than growing that
//!   already-huge, heavily-matched enum with a fork-only event just to
//!   restore the broadcast, resolution here is a **direct call**: whichever
//!   frontend materializer decided a request's outcome (e.g.
//!   `TuiOrchestrationModel::fail_child_request`) calls [`resolve_error`]
//!   directly on the `ModelHandle<StartAgentExecutor>` it already holds
//!   (it received the `CreateAgent` event from that same handle). This is a
//!   deliberate architectural simplification versus the pin, safe only
//!   because every resolution in this fork happens synchronously within the
//!   same dispatch -- there is no cross-cutting consumer (like the pin's
//!   `OrchestrationEventStreamer`) that needs the decoupled broadcast.
//! - **Success resolution.** Nothing in this fork can currently resolve a
//!   `StartAgent` request as `Started`: that requires native local-Oz child
//!   materialization (the pin's `begin_local_oz_child_launch` /
//!   `register_local_oz_child_session`), which needs
//!   `prepare_local_oz_child_launch` -- and that pin function, despite its
//!   name, creates the child's task via `ServerApiProvider::as_ref(ctx)
//!   .get_ai_client().create_agent_task(..)`, i.e. it is cloud-coupled even
//!   in "local" mode. Building a genuinely local replacement is future
//!   work (see `crates/warp_tui/src/orchestration_model.rs`'s module doc),
//!   so only the failure path is wired for now.
//!
//! [`resolve_error`]: StartAgentExecutor::resolve_error
use std::collections::HashMap;

// `SingletonEntity` is what provides `BlocklistAIHistoryModel::as_ref(ctx)`.
// Without it in scope the compiler falls back to `AsRef` and reports the
// confusing "trait bounds were not satisfied" rather than a missing import.
use warpui::{Entity, ModelContext, SingletonEntity};

use crate::ai::agent::conversation::{AIConversationId, ConversationStatus};
use crate::ai::agent::{LifecycleEventType, StartAgentExecutionMode};
use crate::ai::blocklist::history_model::BlocklistAIHistoryModel;

/// Per-request outcome of a StartAgent dispatch.
#[derive(Debug, Clone)]
pub enum StartAgentOutcome {
    Started {
        agent_id: String,
    },
    /// An error occurred while starting the agent.
    Error(String),
}

/// Opaque, monotonically increasing request identifier.
/// Disambiguates parallel in-flight StartAgent requests.
#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, Default)]
pub struct StartAgentRequestId(u64);

impl StartAgentRequestId {
    #[cfg(test)]
    pub const fn from_raw_for_test(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Clone)]
pub struct StartAgentRequest {
    pub id: StartAgentRequestId,
    pub name: String,
    pub prompt: String,
    pub execution_mode: StartAgentExecutionMode,
    pub lifecycle_subscription: Option<Vec<LifecycleEventType>>,
    pub parent_conversation_id: AIConversationId,
    pub parent_run_id: Option<String>,
}

struct PendingStartAgent {
    sender: async_channel::Sender<StartAgentOutcome>,
}

/// One per session surface (mirroring the pin), not a singleton -- a caller
/// creates its own via `ctx.add_model(StartAgentExecutor::new)`.
pub struct StartAgentExecutor {
    pending: HashMap<StartAgentRequestId, PendingStartAgent>,
    next_request_id: u64,
}

impl StartAgentExecutor {
    pub fn new(_ctx: &mut ModelContext<Self>) -> Self {
        Self {
            pending: HashMap::new(),
            next_request_id: 0,
        }
    }

    fn next_request_id(&mut self) -> StartAgentRequestId {
        let id = self.next_request_id;
        self.next_request_id = self.next_request_id.wrapping_add(1);
        StartAgentRequestId(id)
    }

    /// Dispatch a pre-validated StartAgent request. Returns a receiver
    /// for the resulting [`StartAgentOutcome`].
    #[allow(clippy::too_many_arguments)]
    pub fn dispatch(
        &mut self,
        name: String,
        prompt: String,
        execution_mode: StartAgentExecutionMode,
        lifecycle_subscription: Option<Vec<LifecycleEventType>>,
        parent_conversation_id: AIConversationId,
        parent_run_id: Option<String>,
        ctx: &mut ModelContext<Self>,
    ) -> async_channel::Receiver<StartAgentOutcome> {
        let (sender, receiver) = async_channel::bounded(1);
        let request_id = self.next_request_id();
        self.pending
            .insert(request_id, PendingStartAgent { sender });
        ctx.emit(StartAgentExecutorEvent::CreateAgent(Box::new(
            StartAgentRequest {
                id: request_id,
                name,
                prompt,
                execution_mode,
                lifecycle_subscription,
                parent_conversation_id,
                parent_run_id,
            },
        )));
        receiver
    }

    /// Resolves a pending request as failed. Called directly by whichever
    /// frontend materializer determined the child could not be launched --
    /// see this module's doc comment for why this fork resolves directly
    /// rather than through a history-event broadcast. `conversation_id` is
    /// the (possibly already-deleted-by-the-time-you-read-this) ephemeral
    /// conversation the caller created to hold the failure, used only to
    /// decide whether it needs cleaning up.
    pub fn resolve_error(
        &mut self,
        request_id: StartAgentRequestId,
        conversation_id: AIConversationId,
        message: String,
        ctx: &mut ModelContext<Self>,
    ) {
        let Some(pending) = self.pending.remove(&request_id) else {
            return;
        };
        let _ = pending.sender.try_send(StartAgentOutcome::Error(message));
        let should_cleanup = BlocklistAIHistoryModel::as_ref(ctx)
            .conversation(&conversation_id)
            .is_some_and(|conversation| should_cleanup_failed_child_launch(conversation.status()));
        if should_cleanup {
            ctx.emit(StartAgentExecutorEvent::CleanupFailedChildLaunch { conversation_id });
        }
    }
}

/// Whether a child that failed before launch should have its hidden pane and
/// conversation cleaned up. Only terminal launch failures qualify; recoverable
/// `Blocked` startup states (e.g. awaiting GitHub auth) and non-terminal
/// `TransientError` (a recovery is in flight) keep their chip so the user can
/// resolve them or let the retry complete. Ported verbatim from the pin --
/// `ConversationStatus` has the same variants here.
fn should_cleanup_failed_child_launch(status: &ConversationStatus) -> bool {
    match status {
        ConversationStatus::Error | ConversationStatus::Cancelled => true,
        ConversationStatus::Blocked { .. }
        | ConversationStatus::InProgress
        | ConversationStatus::TransientError
        | ConversationStatus::Success
        | ConversationStatus::WaitingForEvents => false,
    }
}

impl Entity for StartAgentExecutor {
    type Event = StartAgentExecutorEvent;
}

pub enum StartAgentExecutorEvent {
    CreateAgent(Box<StartAgentRequest>),
    /// A child agent failed at the launch stage (never started). The owning
    /// materializer removes its hidden pane/session and conversation so the
    /// orchestration pill bar does not retain a dead chip.
    CleanupFailedChildLaunch {
        conversation_id: AIConversationId,
    },
}
