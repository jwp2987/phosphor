//! Demultiplexes one agent-event stream into parent/child roles.
//!
//! A "family" stream is a subscription that carries a parent run's own events
//! *and* its children's on one channel. [`classify_family_event`] is the single
//! predicate that decides, for one [`AgentRunEvent`], which role it plays
//! relative to the subscriber's own run id: the parent's own inbox/lifecycle
//! traffic, a child discovery, a child session link, a child lifecycle
//! transition, or nothing actionable.
//!
//! Local only. [`AgentRunEvent`] is this fork's own type
//! (`app/src/ai/agent_events/mod.rs`), fed from a BYOP provider's SSE stream
//! via `ai/agent_sdk/driver/harness/claude_code/parent_bridge.rs` -- not from
//! Warp. Nothing here touches `ServerApi`, `AIClient` or `warp_graphql`.
//!
//! # Scope of the fork's use today
//!
//! The only consumer here, `parent_bridge.rs`'s `MessageBridgeEventConsumer`,
//! subscribes to a *single* run id
//! (`AgentEventDriverConfig::bounded_run_ids(run_ids, since_sequence)`, called
//! with a one-element vector), so it only ever reaches
//! [`FamilyEvent::ParentSelf`] and [`FamilyEvent::Opaque`] in practice; it
//! narrows `ParentSelf` further to [`EVENT_NEW_MESSAGE`] because a message
//! bridge must not stage the run's own lifecycle events as lead-agent
//! messages. The child arms are exercised by tests and wait on a genuine
//! multi-run subscriber. Keeping the whole routing table in one place is the
//! point: it is what makes every consumer agree on which wire strings mean
//! what, rather than each one open-coding its own.
use warp_multi_agent_api as api;

use crate::ai::agent_events::AgentRunEvent;

/// Wire `event_type` for a run's own inbox message events.
pub(crate) const EVENT_NEW_MESSAGE: &str = "new_message";
/// Wire `event_type` emitted on a PARENT run when a child task is created
/// (`AddTask` with `parent_run_id`); the child run id is carried in `ref_id`.
pub(crate) const EVENT_CHILD_AGENT_STARTED: &str = "child_agent_started";
/// Wire `event_type` emitted on a CHILD run when its sandbox session links;
/// the session UUID is carried in `ref_id`.
pub(crate) const EVENT_RUN_SESSION_LINKED: &str = "run_session_linked";

/// Classification of a single event from a parent-family stream. Produced by
/// [`classify_family_event`].
#[derive(Debug, PartialEq)]
pub(crate) enum FamilyEvent {
    /// Inbox message or lifecycle event on the subscriber's own run.
    ParentSelf(AgentRunEvent),
    /// `child_agent_started` on the parent run; the child run id is the
    /// event's `ref_id`.
    ChildStarted { child_run_id: String },
    /// `run_session_linked` on a child run; the session UUID is the event's
    /// `ref_id`, letting a tracker fill in `session_id` without a fetch.
    ChildSessionLinked {
        child_run_id: String,
        session_uuid: String,
    },
    /// A recognised lifecycle event on a child run.
    ChildLifecycle {
        child_run_id: String,
        kind: api::LifecycleEventType,
    },
    /// Anything else (unrecognised type, malformed discovery/session event):
    /// advances the cursor only, for forward compatibility.
    Opaque,
}

/// Classifies one family-stream event relative to the subscriber's own
/// `self_run_id`. Discovery (`child_agent_started`) is recognised only on the
/// subscriber's own run; session links and lifecycle events are recognised only
/// on other (child) runs; the subscriber's own inbox/lifecycle events become
/// [`FamilyEvent::ParentSelf`]; everything else is [`FamilyEvent::Opaque`].
pub(crate) fn classify_family_event(event: &AgentRunEvent, self_run_id: &str) -> FamilyEvent {
    let is_self = event.run_id == self_run_id;
    match (is_self, event.event_type.as_str()) {
        (true, EVENT_CHILD_AGENT_STARTED) => match event.ref_id.as_deref() {
            Some(child_run_id) if !child_run_id.is_empty() => FamilyEvent::ChildStarted {
                child_run_id: child_run_id.to_string(),
            },
            // A discovery event with no child run id is unusable.
            _ => FamilyEvent::Opaque,
        },
        (false, EVENT_RUN_SESSION_LINKED) => match event.ref_id.as_deref() {
            Some(session_uuid) if !session_uuid.is_empty() => FamilyEvent::ChildSessionLinked {
                child_run_id: event.run_id.clone(),
                session_uuid: session_uuid.to_string(),
            },
            _ => FamilyEvent::Opaque,
        },
        (false, event_type) => match lifecycle_event_type_from_wire(event_type) {
            Some(kind) => FamilyEvent::ChildLifecycle {
                child_run_id: event.run_id.clone(),
                kind,
            },
            // A child `new_message` or any unrecognised type: not actionable
            // by a tracker (a viewer drops it, the owner has no delivery path
            // for another run's inbox).
            None => FamilyEvent::Opaque,
        },
        (true, EVENT_NEW_MESSAGE) => FamilyEvent::ParentSelf(event.clone()),
        (true, event_type) => {
            // The subscriber's own lifecycle events are ParentSelf;
            // unrecognised self events advance the cursor only.
            if lifecycle_event_type_from_wire(event_type).is_some() {
                FamilyEvent::ParentSelf(event.clone())
            } else {
                FamilyEvent::Opaque
            }
        }
    }
}

/// Maps a wire `event_type` string onto an [`api::LifecycleEventType`],
/// recognising the legacy names alongside the canonical task-state-aligned
/// ones. Keeping the table in one place means every family consumer agrees on
/// which legacy variants are recognised.
pub(crate) fn lifecycle_event_type_from_wire(event_type: &str) -> Option<api::LifecycleEventType> {
    match event_type {
        // New canonical event types aligned with task states.
        "run_in_progress" => Some(api::LifecycleEventType::InProgress),
        "run_succeeded" => Some(api::LifecycleEventType::Succeeded),
        "run_failed" => Some(api::LifecycleEventType::Failed),
        // Legacy event types mapped to new variants for backward compat.
        #[allow(deprecated)]
        "run_started" => Some(api::LifecycleEventType::InProgress),
        #[allow(deprecated)]
        "run_idle" => Some(api::LifecycleEventType::Succeeded),
        #[allow(deprecated)]
        "run_restarted" => Some(api::LifecycleEventType::InProgress),
        "run_errored" => Some(api::LifecycleEventType::Errored),
        "run_cancelled" => Some(api::LifecycleEventType::Cancelled),
        "run_blocked" => Some(api::LifecycleEventType::Blocked),
        _ => None,
    }
}
