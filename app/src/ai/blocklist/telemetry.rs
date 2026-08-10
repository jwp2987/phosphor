//! Telemetry event types consumed by the orchestration pill bar.
//!
//! **Not the pin's full catalog.** Zap's telemetry *sending* has been
//! physically removed -- see `crates/warp_core/src/telemetry.rs`'s
//! `send_telemetry_from_ctx!` family and `DECLINED.md`'s "Telemetry and
//! crash reporting" entry. Those macros are untyped compatibility shims:
//! they reference `$event` inside an `if false` branch purely so call sites
//! keep type-checking, with no `TelemetryEvent`/`TelemetryEventDesc` trait
//! bound on `$event` at all. So unlike the pin's `blocklist::telemetry.rs`,
//! this module carries none of the event-catalog registration machinery
//! (`TelemetryEvent`/`TelemetryEventDesc` impls,
//! `warp_core::register_telemetry_event!`, `EnablementState`,
//! `EnumDiscriminants`) -- that machinery does not exist anywhere in this
//! fork. (This fork instead keeps the *old* monolithic
//! `crate::server::telemetry::TelemetryEvent` enum as its own type shell for
//! most call sites -- a different, pre-existing architecture, not touched
//! here.) What's left is just the plain data types the pill bar constructs
//! and hands to `send_telemetry_from_ctx!`.
//!
//! `PillBarInteraction` and `TeamAgentCommunicationFailed` are ported. The
//! pin's other four variants (`PlanConfigApprovalToggled`,
//! `RunAgentsCardDecision`, `OrchestrationEntered`, `AgentProposedConfig`)
//! and their support types (`OrchestrationExecutionModeKind`,
//! `OrchestrationHarnessKind`, `orchestration_modified_field`, ...) are used
//! exclusively by `controller.rs` / `inline_action/run_agents_card_view.rs` /
//! `document/orchestration_config_block.rs` -- none of which are in scope
//! here. One of them doesn't even compile in this fork as-is:
//! `OrchestrationExecutionModeKind::from_run_agents` takes
//! `ai::agent::action::RunAgentsExecutionMode`, a Step 3 (`RunAgents`) type
//! that does not exist here yet.
//!
//! `TeamAgentCommunicationFailed` is a plain data type (no dependency on
//! `RunAgents`), so it's ported here alongside
//! `AIAgentActionType::SendMessageToAgent`. Its only consumer in the pin,
//! `action_model/execute/send_message.rs`, is genuinely cloud-backed
//! (`crate::server::server_api::ServerApiProvider`,
//! `crate::server::server_api::ai::{SendAgentMessageRequest,
//! SendAgentMessageResponse}` -- both cloud markers) and stays unported;
//! nothing here constructs `TeamAgentCommunicationFailedEvent` yet.

use serde::Serialize;

use crate::ai::agent::conversation::AIConversationId;

#[derive(Debug)]
pub(crate) enum BlocklistOrchestrationTelemetryEvent {
    PillBarInteraction(PillBarInteractionEvent),
    TeamAgentCommunicationFailed(TeamAgentCommunicationFailedEvent),
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub(crate) enum TeamAgentCommunicationKind {
    Message,
    LifecycleEvent,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub(crate) enum TeamAgentCommunicationTransport {
    Local,
    ServerApi,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub(crate) enum TeamAgentOrchestrationVersion {
    V1,
    V2,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub(crate) enum TeamAgentCommunicationFailureReason {
    InvalidLifecycleEventType,
    MissingSourceConversation,
    MissingSourceIdentifier,
    UnknownAgent,
    NoTargets,
    RequestFailed,
}

#[derive(Debug, Serialize)]
pub(crate) struct TeamAgentCommunicationFailedEvent {
    pub communication_kind: TeamAgentCommunicationKind,
    pub transport: TeamAgentCommunicationTransport,
    pub orchestration_version: TeamAgentOrchestrationVersion,
    pub failure_reason: TeamAgentCommunicationFailureReason,
    pub source_conversation_id: AIConversationId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lifecycle_event_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PillBarPillKind {
    Orchestrator,
    Child,
}

/// Concrete user actions against an orchestration pill bar entry.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PillBarActionKind {
    /// User clicked the pill body. See `switch_outcome` for what
    /// happened next.
    Switch,
    OpenInNewPane,
    OpenInNewTab,
    // No `ViewInOz` variant: the pin's "View in Oz" menu item opens a
    // Warp-cloud web URL (`{oz_root_url}/runs/{run_id}`) to view the run
    // online. `oz_root_url` doesn't exist in this fork -- there is no cloud
    // web app for a BYOP-local run to be viewed in -- so the menu item (and
    // its telemetry kind) is dropped entirely rather than gated on a URL
    // that can never resolve. See `orchestration_pill_bar.rs`'s
    // `open_menu_for` for the corresponding menu-item omission.
    /// User picked "Focus pane" from a pill's 3-dot menu. Distinct
    /// from a pill-body click that resolves to the same outcome
    /// (those are `Switch` with `switch_outcome = focused_existing_pane`).
    FocusOpenedConversation,
    Stop,
    Kill,
    TogglePinOn,
    TogglePinOff,
    OpenMenu,
}

/// Outcome of a pill-body click. Closed enum so future navigation
/// outcomes can be added without splitting `Switch` into multiple
/// action variants again.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PillSwitchOutcome {
    /// Pill click navigated within the current pane.
    SwitchedInPlace,
    /// Target conversation was already owned by another visible
    /// terminal view; focus moved there instead of switching in place.
    FocusedExistingPane,
}

#[derive(Debug, Serialize)]
pub(crate) struct PillBarInteractionEvent {
    pub action: PillBarActionKind,
    pub pill_kind: PillBarPillKind,
    pub total_pills: usize,
    pub total_pinned: usize,
    /// The orchestrator that hosts the pill bar.
    pub source_conversation_id: AIConversationId,
    /// The pill the action targets.
    pub target_conversation_id: AIConversationId,
    /// Present only when `action == Switch`. Distinguishes whether the
    /// pill-body click navigated within the current pane or moved
    /// focus to an existing pane already owning the conversation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub switch_outcome: Option<PillSwitchOutcome>,
}
