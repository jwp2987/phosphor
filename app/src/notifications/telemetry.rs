//! Telemetry events for the in-app notification mailbox / toast stack.
//!
//! This is a minimally-trimmed version of `AgentManagementTelemetryEvent`,
//! which was deleted along with 002ce467's cloud-removal, keeping only the
//! variant actually still used by the notification center
//! (`item_rendering.rs`) — the artifact click event + tombstone no longer
//! exist, but the schema is kept for backward compatibility/future rebuilds.

use serde::Serialize;

/// Notification artifact type (used for telemetry).
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactType {
    Plan,
    Branch,
    PullRequest,
}

/// Telemetry events related to the notification center.
#[derive(Serialize, Debug)]
pub enum NotificationsTelemetryEvent {
    /// The user clicked an artifact button (plan / branch / PR) in a notification item
    ArtifactClicked { artifact_type: ArtifactType },
}
