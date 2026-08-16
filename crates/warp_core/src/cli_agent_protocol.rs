//! The CLI-agent notification protocol shared by every producer and consumer of
//! OSC 777 `notify` events in this workspace.
//!
//! The GUI listener (`app/src/terminal/cli_agent_sessions/event`) consumes these
//! events; this fork's own TUI
//! (`crates/warp_tui/src/cli_agent_osc_event_publisher.rs`) produces them. The
//! wire type used to live privately in the GUI's `event/v1.rs`, which meant the
//! TUI could not emit against it without duplicating the schema.
//!
//! ## Why the identifiers below still say "warp"
//!
//! `warp://cli-agent`, `WARP_CLI_AGENT_PROTOCOL_VERSION` and
//! `WARP_CLIENT_VERSION` are **protocol**, not branding. Third-party CLI-agent
//! plugins (the Claude, OpenCode, Gemini, Auggie and Pi integrations, plus any
//! user-written hook) already speak this protocol and match these exact bytes;
//! renaming them would silently stop every existing plugin from being
//! recognised, in both directions. They are therefore kept verbatim from the
//! pinned oracle even though this fork's user-facing name is Phosphor. Nothing
//! here is rendered to a user — see `display_name()` and the notification
//! summaries in the TUI publisher for the strings that are.

use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;

/// Sentinel title that identifies structured CLI-agent events sent via OSC 777.
pub const CLI_AGENT_NOTIFICATION_SENTINEL: &str = "warp://cli-agent";

/// Schema version emitted by the current CLI-agent notification protocol.
pub const CLI_AGENT_PROTOCOL_VERSION: u32 = 1;

/// Environment variable that advertises the host's CLI-agent protocol version.
pub const WARP_CLI_AGENT_PROTOCOL_VERSION_ENV: &str = "WARP_CLI_AGENT_PROTOCOL_VERSION";

/// Environment variable that identifies the hosting terminal's client version.
pub const WARP_CLIENT_VERSION_ENV: &str = "WARP_CLIENT_VERSION";

/// Wire representation of a structured CLI-agent notification.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CLIAgentNotification {
    pub v: Option<u32>,
    pub agent: Option<String>,
    pub event: String,
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    pub project: Option<String>,
    pub query: Option<String>,
    pub response: Option<String>,
    pub transcript_path: Option<String>,
    pub summary: Option<String>,
    pub tool_name: Option<String>,
    pub tool_input: Option<serde_json::Value>,
    pub plugin_version: Option<String>,
    pub error_type: Option<String>,
}

impl CLIAgentNotification {
    pub fn new(agent: impl Into<String>, event: impl Into<String>) -> Self {
        Self {
            v: Some(CLI_AGENT_PROTOCOL_VERSION),
            agent: Some(agent.into()),
            event: event.into(),
            session_id: None,
            cwd: None,
            project: None,
            query: None,
            response: None,
            transcript_path: None,
            summary: None,
            tool_name: None,
            tool_input: None,
            plugin_version: None,
            error_type: None,
        }
    }
}
