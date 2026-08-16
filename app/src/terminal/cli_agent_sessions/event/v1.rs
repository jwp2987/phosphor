use warp_core::cli_agent_protocol::CLIAgentNotification;

use crate::terminal::CLIAgent;

use super::{CLIAgentEvent, CLIAgentEventPayload, CLIAgentEventSource, CLIAgentEventType};

/// Resolves a CLI agent from the `"agent"` string in a CLI agent event.
/// Returns `None` if the string doesn't match any known agent.
///
/// Matches against *every* prefix an agent answers to, not just its canonical
/// first one, which is what the pinned oracle does. Agents that ship several
/// binaries under one identity — `vibe`/`vibe-acp`, `deepseek`/`deepseek-tui`,
/// and this fork's own TUI, whose `"agent"` value on the wire is `"warp-tui"`
/// while its canonical prefix is `"warp"` — would otherwise fall through to
/// `CLIAgent::Unknown`. `CLIAgent::Unknown` needs no special case: its prefix
/// list is empty, so `contains` can never match it.
fn resolve_agent(agent: &str) -> Option<CLIAgent> {
    enum_iterator::all::<CLIAgent>().find(|candidate| candidate.command_prefixes().contains(&agent))
}

pub(super) fn parse(body: &str) -> Option<CLIAgentEvent> {
    let raw: CLIAgentNotification = serde_json::from_str(body).ok()?;

    let event = match raw.event.as_str() {
        "session_start" => CLIAgentEventType::SessionStart,
        "prompt_submit" => CLIAgentEventType::PromptSubmit,
        "tool_complete" => CLIAgentEventType::ToolComplete,
        "stop" => CLIAgentEventType::Stop,
        "permission_request" => CLIAgentEventType::PermissionRequest,
        "permission_replied" => CLIAgentEventType::PermissionReplied,
        "question_asked" => CLIAgentEventType::QuestionAsked,
        "idle_prompt" => CLIAgentEventType::IdlePrompt,
        other => CLIAgentEventType::Unknown(other.to_string()),
    };

    let tool_input_preview = raw.tool_input.as_ref().and_then(|val| {
        val.get("command")
            .or_else(|| val.get("file_path"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    });

    let agent = raw
        .agent
        .as_deref()
        .and_then(resolve_agent)
        .unwrap_or(CLIAgent::Unknown);

    Some(CLIAgentEvent {
        v: raw.v.unwrap_or(1),
        agent,
        event,
        session_id: raw.session_id,
        cwd: raw.cwd,
        project: raw.project,
        payload: CLIAgentEventPayload {
            query: raw.query,
            response: raw.response,
            transcript_path: raw.transcript_path,
            summary: raw.summary,
            tool_name: raw.tool_name,
            tool_input_preview,
            plugin_version: raw.plugin_version,
        },
        source: CLIAgentEventSource::RichPlugin,
    })
}
