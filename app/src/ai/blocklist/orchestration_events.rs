//! Local queue for agent-to-agent messages and lifecycle events.
//!
//! Events between an orchestrator and its local child agents (both running
//! as processes on this machine — see
//! `app/src/pane_group/pane/local_harness_launch.rs`) are delivered by
//! embedding them into the target conversation's next `AIAgentInput` turn
//! and reading them back out of that turn's `AIAgentOutputMessageType`
//! (`MessagesReceivedFromAgents` / `EventsFromAgents`), the same mechanism
//! used for every other agent turn regardless of which BYOP backend is
//! configured. `OrchestrationEventService` is the in-memory bookkeeping
//! around that: it queues undelivered events per target conversation,
//! coalesces supersedable lifecycle transitions, enforces a cap so a stuck
//! recipient can't grow the queue unbounded, and tracks delivery
//! confirmation ("echo") once a turn's output shows the event was seen.
//! None of this talks to a remote service — "echo" here means "came back
//! out in this conversation's own output", not a network round trip.
use std::collections::{HashMap, HashSet};

use agent_proto as api;
use warpui::{Entity, ModelContext, SingletonEntity};

use super::history_model::{
    BlocklistAIHistoryEvent, BlocklistAIHistoryModel, ConversationStatusUpdate,
};
use crate::ai::agent::conversation::{AIConversationId, ConversationStatus};
use crate::ai::agent::task::TaskId;
use crate::ai::agent::{
    AIAgentExchangeId, AIAgentInput, AIAgentOutputMessageType, LifecycleEventType,
};

/// Stage associated with a lifecycle error detail.
/// This keeps persisted/runtime metadata consistent across API payloads and DB rows.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[allow(dead_code)] // persisted or derived enum
pub enum LifecycleEventDetailStage {
    Runtime,
}

impl LifecycleEventDetailStage {
    /// Canonical lowercase representation used in persistence/API payloads.
    #[allow(dead_code)] // used by sibling unused code or shared name
    fn as_str(self) -> &'static str {
        match self {
            Self::Runtime => "runtime",
        }
    }
}

/// Type-specific queued data.
#[derive(Debug, Clone)]
#[allow(dead_code)] // used by sibling unused code or shared name
pub enum PendingEventDetail {
    Message {
        message_id: String,
        #[allow(dead_code)] // field: set at construction or persisted
        addresses: Vec<String>,
        #[allow(dead_code)] // field: set at construction or persisted
        subject: String,
        #[allow(dead_code)] // field: set at construction or persisted
        message_body: String,
    },
    Lifecycle {
        event: api::AgentEvent,
    },
}

/// A queued event consumed by the controller.
#[derive(Debug, Clone)]
#[allow(dead_code)] // used by sibling unused code or shared name
pub struct PendingEvent {
    pub event_id: String,
    #[allow(dead_code)] // field: set at construction or persisted
    pub source_agent_id: String,
    pub attempt_count: i32,
    pub detail: PendingEventDetail,
}

#[allow(dead_code)] // used by sibling unused code or shared name
pub enum OrchestrationEventServiceEvent {
    /// Signals that a conversation may have pending orchestration events
    /// ready to drain.
    #[allow(dead_code)] // used by sibling unused code or shared name
    EventsReady { conversation_id: AIConversationId },
}

/// Synchronous state manager for orchestration event queuing, delivery tracking, and readiness detection.
#[allow(dead_code)] // used only by tests
pub struct OrchestrationEventService {
    pending_events: HashMap<AIConversationId, Vec<PendingEvent>>,
    #[allow(dead_code)] // field: set at construction or persisted
    awaiting_server_echo_events: HashMap<AIConversationId, Vec<PendingEvent>>,
    #[allow(dead_code)] // field: set at construction or persisted
    conversation_statuses: HashMap<AIConversationId, ConversationStatus>,
}

#[allow(dead_code)] // members used on other cfgs or tests
impl OrchestrationEventService {
    pub fn new(ctx: &mut ModelContext<Self>) -> Self {
        let history_model = BlocklistAIHistoryModel::handle(ctx);
        ctx.subscribe_to_model(&history_model, move |me, event, ctx| {
            me.handle_history_event(event, ctx);
        });
        Self::new_without_subscriptions()
    }

    fn new_without_subscriptions() -> Self {
        Self {
            pending_events: HashMap::new(),
            awaiting_server_echo_events: HashMap::new(),
            conversation_statuses: HashMap::new(),
        }
    }

    pub fn handle_history_event(
        &mut self,
        event: &BlocklistAIHistoryEvent,
        ctx: &mut ModelContext<Self>,
    ) {
        match event {
            BlocklistAIHistoryEvent::UpdatedConversationStatus {
                conversation_id,
                update,
                ..
            } => {
                let is_restored = matches!(update, ConversationStatusUpdate::Restored);
                self.on_conversation_status_updated(*conversation_id, is_restored, ctx)
            }
            BlocklistAIHistoryEvent::UpdatedStreamingExchange {
                conversation_id,
                exchange_id,
                ..
            } => self.confirm_delivery_from_exchange(*conversation_id, *exchange_id, ctx),
            BlocklistAIHistoryEvent::StartedNewConversation {
                new_conversation_id,
                ..
            } => self.sync_conversation_status(*new_conversation_id, ctx),
            BlocklistAIHistoryEvent::RestoredConversations {
                conversation_ids, ..
            } => {
                for conversation_id in conversation_ids {
                    self.sync_conversation_status(*conversation_id, ctx);
                }
            }
            BlocklistAIHistoryEvent::RemoveConversation {
                conversation_id, ..
            }
            | BlocklistAIHistoryEvent::DeletedConversation {
                conversation_id, ..
            } => {
                self.pending_events.remove(conversation_id);
                self.awaiting_server_echo_events.remove(conversation_id);
                self.conversation_statuses.remove(conversation_id);
            }
            _ => {}
        }
    }

    fn sync_conversation_status(
        &mut self,
        conversation_id: AIConversationId,
        ctx: &ModelContext<Self>,
    ) {
        let Some(conversation) =
            BlocklistAIHistoryModel::as_ref(ctx).conversation(&conversation_id)
        else {
            self.conversation_statuses.remove(&conversation_id);
            return;
        };
        self.conversation_statuses
            .insert(conversation_id, conversation.status().clone());
    }

    fn on_conversation_status_updated(
        &mut self,
        conversation_id: AIConversationId,
        is_restored: bool,
        ctx: &mut ModelContext<Self>,
    ) {
        let current_status = {
            let Some(conversation) =
                BlocklistAIHistoryModel::as_ref(ctx).conversation(&conversation_id)
            else {
                self.conversation_statuses.remove(&conversation_id);
                return;
            };
            conversation.status().clone()
        };

        self.conversation_statuses
            .insert(conversation_id, current_status.clone());
        let has_pending = self
            .pending_events
            .get(&conversation_id)
            .is_some_and(|events| !events.is_empty());
        // Re-fire EventsReady whenever the conversation reaches a status
        // that `conversation_ready_for_pending_events` would accept, so
        // events queued while the stream was in flight drain as soon as
        // the stream finishes — either to `Success` or to
        // `WaitingForEvents` via a `wait_for_events` yield.
        if !is_restored
            && matches!(
                &current_status,
                ConversationStatus::Success | ConversationStatus::WaitingForEvents
            )
            && has_pending
        {
            ctx.emit(OrchestrationEventServiceEvent::EventsReady { conversation_id });
        }
    }

    #[cfg(any(test, not(target_family = "wasm")))]
    pub fn has_pending_events(&self, conversation_id: AIConversationId) -> bool {
        self.pending_events
            .get(&conversation_id)
            .is_some_and(|events| !events.is_empty())
    }

    /// Scans the exchange output for orchestration IDs echoed back in this
    /// conversation's own turn output, then clears matching entries from
    /// awaiting_server_echo_events.
    fn confirm_delivery_from_exchange(
        &mut self,
        conversation_id: AIConversationId,
        exchange_id: AIAgentExchangeId,
        ctx: &ModelContext<Self>,
    ) {
        if !self
            .awaiting_server_echo_events
            .contains_key(&conversation_id)
        {
            return;
        }

        let Some(conversation) =
            BlocklistAIHistoryModel::as_ref(ctx).conversation(&conversation_id)
        else {
            return;
        };
        let Some(exchange) = conversation.exchange_with_id(exchange_id) else {
            return;
        };

        let mut echoed_message_ids = Vec::new();
        let mut echoed_lifecycle_event_ids = Vec::new();
        if let Some(output) = exchange.output_status.output() {
            for msg in &output.get().messages {
                match &msg.message {
                    AIAgentOutputMessageType::MessagesReceivedFromAgents { messages } => {
                        for received in messages {
                            if !received.message_id.is_empty() {
                                echoed_message_ids.push(received.message_id.clone());
                            }
                        }
                    }
                    AIAgentOutputMessageType::EventsFromAgents { event_ids } => {
                        for id in event_ids {
                            if !id.is_empty() {
                                echoed_lifecycle_event_ids.push(id.clone());
                            }
                        }
                    }
                    AIAgentOutputMessageType::Text(_)
                    | AIAgentOutputMessageType::Reasoning { .. }
                    | AIAgentOutputMessageType::Summarization { .. }
                    | AIAgentOutputMessageType::Subagent(_)
                    | AIAgentOutputMessageType::Action(_)
                    | AIAgentOutputMessageType::TodoOperation(_)
                    | AIAgentOutputMessageType::WebSearch(_)
                    | AIAgentOutputMessageType::WebFetch(_)
                    | AIAgentOutputMessageType::CommentsAddressed { .. }
                    | AIAgentOutputMessageType::DebugOutput { .. }
                    | AIAgentOutputMessageType::ArtifactCreated(_)
                    | AIAgentOutputMessageType::SkillInvoked(_)
                    | AIAgentOutputMessageType::RejectedToolCall { .. } => {}
                }
            }
        }

        if !echoed_message_ids.is_empty() || !echoed_lifecycle_event_ids.is_empty() {
            self.acknowledge_delivery_from_server_echo(
                conversation_id,
                &echoed_message_ids,
                &echoed_lifecycle_event_ids,
            );
        }
    }

    /// Clears awaiting_server_echo_events entries that match the given IDs.
    fn acknowledge_delivery_from_server_echo(
        &mut self,
        conversation_id: AIConversationId,
        echoed_message_ids: &[String],
        echoed_lifecycle_event_ids: &[String],
    ) {
        if echoed_message_ids.is_empty() && echoed_lifecycle_event_ids.is_empty() {
            return;
        }

        let echoed_message_ids: HashSet<&str> =
            echoed_message_ids.iter().map(String::as_str).collect();
        let echoed_lifecycle_event_ids: HashSet<&str> = echoed_lifecycle_event_ids
            .iter()
            .map(String::as_str)
            .collect();
        let should_remove_entry = {
            let Some(awaiting_events) = self.awaiting_server_echo_events.get_mut(&conversation_id)
            else {
                return;
            };

            awaiting_events.retain(|pending_event| {
                let was_echoed = did_event_round_trip_through_server(
                    pending_event,
                    &echoed_message_ids,
                    &echoed_lifecycle_event_ids,
                );
                !was_echoed
            });

            awaiting_events.is_empty()
        };

        if should_remove_entry {
            self.awaiting_server_echo_events.remove(&conversation_id);
        }
    }
}

#[allow(dead_code)] // used only by tests
fn did_event_round_trip_through_server(
    pending_event: &PendingEvent,
    echoed_message_ids: &HashSet<&str>,
    echoed_lifecycle_event_ids: &HashSet<&str>,
) -> bool {
    match &pending_event.detail {
        PendingEventDetail::Message { message_id, .. } => {
            echoed_message_ids.contains(message_id.as_str())
        }
        PendingEventDetail::Lifecycle { event } => {
            echoed_lifecycle_event_ids.contains(event.event_id.as_str())
        }
    }
}

#[allow(deprecated)]
#[allow(dead_code)] // used only by tests
pub(super) fn lifecycle_event_type_from_proto(
    lifecycle_event: &api::agent_event::LifecycleEvent,
) -> api::LifecycleEventType {
    match lifecycle_event.detail.as_ref() {
        Some(api::agent_event::lifecycle_event::Detail::InProgress(_)) => {
            api::LifecycleEventType::InProgress
        }
        Some(api::agent_event::lifecycle_event::Detail::Succeeded(_)) => {
            api::LifecycleEventType::Succeeded
        }
        Some(api::agent_event::lifecycle_event::Detail::Failed(_)) => {
            api::LifecycleEventType::Failed
        }
        // Legacy detail variants map to new types.
        Some(api::agent_event::lifecycle_event::Detail::Started(_)) => {
            api::LifecycleEventType::InProgress
        }
        Some(api::agent_event::lifecycle_event::Detail::Idle(_)) => {
            api::LifecycleEventType::Succeeded
        }
        Some(api::agent_event::lifecycle_event::Detail::Restarted(_)) => {
            api::LifecycleEventType::InProgress
        }
        Some(api::agent_event::lifecycle_event::Detail::Cancelled(_)) => {
            api::LifecycleEventType::Cancelled
        }
        Some(api::agent_event::lifecycle_event::Detail::Blocked(_)) => {
            api::LifecycleEventType::Blocked
        }
        Some(api::agent_event::lifecycle_event::Detail::Errored(_)) => {
            api::LifecycleEventType::Errored
        }
        None => api::LifecycleEventType::Unspecified,
    }
}

/// True when a pending event is a lifecycle succeeded/in_progress event and
/// therefore eligible to be superseded by a newer lifecycle transition from
/// the same sender.
#[allow(dead_code)] // used by sibling unused code or shared name
fn is_coalescable_lifecycle_pending_event(event: &PendingEvent) -> bool {
    let PendingEventDetail::Lifecycle { event: agent_event } = &event.detail else {
        return false;
    };
    let Some(api::agent_event::Event::LifecycleEvent(lifecycle_event)) = &agent_event.event else {
        return false;
    };
    matches!(
        lifecycle_event_type_from_proto(lifecycle_event),
        api::LifecycleEventType::Succeeded | api::LifecycleEventType::InProgress
    )
}

#[allow(dead_code)] // used only by tests
fn coalesce_lifecycle_events(
    queue: &mut Vec<PendingEvent>,
    new_event: &PendingEvent,
) -> Vec<String> {
    // Remove older supersedable lifecycle events for the same sender as `new_event`.
    // Returns the removed event IDs for callers that need observability/debug assertions.
    // Only coalesce supersedable lifecycle states (succeeded/in_progress). Critical states
    // like errored/cancelled/blocked are retained so recipients don't lose important
    // transitions.
    let PendingEventDetail::Lifecycle {
        event: new_agent_event,
    } = &new_event.detail
    else {
        return vec![];
    };
    let Some(api::agent_event::Event::LifecycleEvent(new_lifecycle_event)) = &new_agent_event.event
    else {
        return vec![];
    };
    let new_type = lifecycle_event_type_from_proto(new_lifecycle_event);
    if !matches!(
        new_type,
        api::LifecycleEventType::Succeeded | api::LifecycleEventType::InProgress
    ) {
        return vec![];
    }

    let mut removed_event_ids = Vec::new();
    queue.retain(|existing| {
        let PendingEventDetail::Lifecycle {
            event: existing_agent_event,
        } = &existing.detail
        else {
            return true;
        };
        let Some(api::agent_event::Event::LifecycleEvent(existing_lifecycle_event)) =
            &existing_agent_event.event
        else {
            return true;
        };
        let existing_type = lifecycle_event_type_from_proto(existing_lifecycle_event);
        let should_remove = existing_lifecycle_event.sender_agent_id
            == new_lifecycle_event.sender_agent_id
            && matches!(
                existing_type,
                api::LifecycleEventType::Succeeded | api::LifecycleEventType::InProgress
            );
        if should_remove {
            removed_event_ids.push(existing.event_id.clone());
        }
        !should_remove
    });
    removed_event_ids
}

/// Enforce an upper bound on pending lifecycle events while preferentially dropping
/// supersedable lifecycle states first, preserving critical transitions.
#[allow(dead_code)] // used only by tests
fn enforce_lifecycle_queue_cap(
    queue: &mut Vec<PendingEvent>,
    max_pending_lifecycle_events: usize,
) -> Vec<String> {
    let mut dropped_event_ids = Vec::new();
    while count_pending_lifecycle_events(queue) > max_pending_lifecycle_events {
        if let Some(index) = queue
            .iter()
            .position(is_coalescable_lifecycle_pending_event)
        {
            dropped_event_ids.push(queue.remove(index).event_id);
        } else {
            // No coalescable items remain; keep critical events.
            break;
        }
    }
    dropped_event_ids
}

/// Count lifecycle entries in a mixed pending queue (message + lifecycle).
#[allow(dead_code)] // used by sibling unused code or shared name
fn count_pending_lifecycle_events(queue: &[PendingEvent]) -> usize {
    queue
        .iter()
        .filter(|event| matches!(event.detail, PendingEventDetail::Lifecycle { .. }))
        .count()
}

/// Increment attempt counts and split events into retryable vs exhausted buckets.
/// Exhaustion is based on `max_retry_attempts` after incrementing this attempt.
#[allow(dead_code)] // used only by tests
fn increment_attempt_and_partition_by_retry_limit(
    mut attempted_events: Vec<PendingEvent>,
    max_retry_attempts: i32,
) -> (Vec<PendingEvent>, Vec<PendingEvent>) {
    for event in &mut attempted_events {
        event.attempt_count += 1;
    }
    attempted_events
        .into_iter()
        .partition(|event| event.attempt_count < max_retry_attempts)
}

impl Default for OrchestrationEventService {
    fn default() -> Self {
        Self::new_without_subscriptions()
    }
}

impl Entity for OrchestrationEventService {
    type Event = OrchestrationEventServiceEvent;
}

impl SingletonEntity for OrchestrationEventService {}

#[cfg(test)]
#[path = "orchestration_events_tests.rs"]
mod tests;
