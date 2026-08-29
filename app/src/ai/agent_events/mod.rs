//! Shared agent-event stream utilities used by third-party harness bridges.

use anyhow::{anyhow, Result};
use async_trait::async_trait;

mod driver;
mod family;
mod message_hydrator;

#[cfg(test)]
pub(crate) use driver::{
    agent_event_backoff, agent_event_failures_exceeded_threshold, AgentEventDriverState,
    DEFAULT_AGENT_EVENT_FAILURES_BEFORE_ERROR_LOG, DEFAULT_AGENT_EVENT_RECONNECT_BACKOFF_STEPS,
    DEFAULT_PERMANENT_ERROR_BACKOFF_STEPS,
};
pub(crate) use driver::{
    run_agent_event_driver, AgentEventConsumer, AgentEventConsumerControlFlow,
    AgentEventDriverConfig, AgentEventSource, AgentEventSourceItem,
};
pub(crate) use family::{classify_family_event, FamilyEvent, EVENT_NEW_MESSAGE};
#[cfg(test)]
pub(crate) use family::{
    lifecycle_event_type_from_wire, EVENT_CHILD_AGENT_STARTED, EVENT_RUN_SESSION_LINKED,
};
pub(crate) use message_hydrator::MessageHydrator;

/// Entry point for the local agent event stream. Zap keeps the interface to support local driver injection; the default implementation disables cloud RTC.
#[cfg_attr(target_family = "wasm", async_trait(?Send))]
#[cfg_attr(not(target_family = "wasm"), async_trait)]
pub(crate) trait AgentEventStreamClient: 'static + Send + Sync {
    async fn stream_agent_events(
        &self,
        run_ids: &[String],
        since_sequence: i64,
    ) -> Result<http_client::EventSourceStream>;
}

pub(crate) struct DisabledAgentEventStreamClient;

#[cfg_attr(target_family = "wasm", async_trait(?Send))]
#[cfg_attr(not(target_family = "wasm"), async_trait)]
impl AgentEventStreamClient for DisabledAgentEventStreamClient {
    async fn stream_agent_events(
        &self,
        _run_ids: &[String],
        _since_sequence: i64,
    ) -> Result<http_client::EventSourceStream> {
        Err(anyhow!(
            "Agent event stream disabled in Phosphor - RTC endpoint is removed"
        ))
    }
}

/// `PartialEq`/`Eq` are load-bearing: `family::FamilyEvent::ParentSelf`
/// carries an `AgentRunEvent` by value and derives `PartialEq`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct AgentRunEvent {
    pub event_type: String,
    pub run_id: String,
    pub ref_id: Option<String>,
    pub execution_id: Option<String>,
    pub occurred_at: String,
    pub sequence: i64,
}

#[cfg(test)]
mod driver_tests;
#[cfg(test)]
mod family_tests;
#[cfg(test)]
mod message_hydrator_tests;
