//! Source-facing helpers that centralize the derivation of the agent-icon shape
//! ([`IconWithStatusVariant`]) from the underlying state models. The invariant the helpers
//! enforce: any single logical agent run renders as the same brand color, glyph, and
//! ambient-vs-local treatment regardless of which surface is rendering it (vertical tabs,
//! notifications mailbox, conversation list / run cards).
//!
//! Deviation from the pinned oracle (`02b53fcd8`): the pin also ships an impure
//! `terminal_view_agent_icon_variant(terminal_view, app)` wrapper that resolves these pure
//! functions' inputs from a live `TerminalView` / `AppContext`, including a branch for a
//! restored cloud transcript (`selected_conversation_server_metadata`) and one for local
//! orchestration children dispatched as server tasks (`selected_conversation_is_local_child`).
//! Those two branches key off Warp's cloud-runner task-dispatch infrastructure
//! (`AgentConversationsModel::get_task_data` backed by ambient/server tasks), which this BYOP
//! fork does not have -- `TerminalView::is_ambient_agent_session` is a hard-coded `false` stub
//! here, and there is no server-metadata-backed transcript restoration. Porting those two
//! branches verbatim would mean inventing new cloud-task wiring, which is out of scope for a
//! `ui_components` icon-consistency port (see `DECLINED.md`'s `RunAgents / cloud-runner
//! orchestration` entry, #290). The pure waterfall below (`agent_icon_variant_from_terminal_inputs`)
//! still carries the full contract, including the pre-dispatch third-party-harness branch, so
//! the moment a caller has real inputs for it, the invariant already holds and is tested.
use warp_cli::agent::Harness;

use crate::ai::agent::conversation::ConversationStatus;
use crate::ai::conversation_entry::AgentConversationEntry;
use crate::terminal::CLIAgent;
use crate::ui_components::icon_with_status::IconWithStatusVariant;

/// Primitive inputs to the terminal-view waterfall, gathered once from the live
/// `TerminalView` / `AppContext` by a caller.
pub(crate) struct TerminalIconInputs {
    pub(crate) is_ambient: bool,
    pub(crate) cli_session: Option<CLISessionInputs>,
    /// Third-party CLI agent for a live ambient run before task data is available (e.g. a
    /// harness pre-dispatch). `None` when there is no such signal available; task-derived
    /// harnesses are handled upstream via [`agent_icon_variant_for_run`].
    pub(crate) selected_third_party_cli_agent: Option<CLIAgent>,
    /// The conversation status that the terminal view would surface in its status-icon slot.
    pub(crate) selected_conversation_status: Option<ConversationStatus>,
    /// Whether the terminal view currently has a selected conversation (ambient or local).
    pub(crate) has_selected_conversation: bool,
}

/// CLI-session-derived inputs for the terminal waterfall.
pub(crate) struct CLISessionInputs {
    pub(crate) agent: CLIAgent,
    /// Whether the session is backed by a plugin listener. Plugin-backed sessions report rich
    /// status; command-detected sessions only know that an agent is running.
    pub(crate) has_listener: bool,
    pub(crate) status: ConversationStatus,
    /// Whether the agent's session handler exposes rich status (plugin-backed handlers report
    /// rich status; handlers that only forward opaque OS notifications do not -- see
    /// `terminal::cli_agent_sessions::listener::session_supports_rich_status`).
    pub(crate) supports_rich_status: bool,
}

/// Pure waterfall from primitive inputs to an [`IconWithStatusVariant`]. `None` means no agent
/// icon should render (a plain terminal with no conversation and no agent activity) -- callers
/// fall back to their own plain-terminal indicator.
///
/// Resolution order:
/// 1. A CLI session with a known (non-`Unknown`) agent wins. Status is only meaningful when
///    the session is plugin-backed and the handler exposes rich status.
/// 2. A live ambient run with a third-party harness selected, before task data is available.
///    `Unknown` is filtered so an unrecognized harness doesn't render as an unbranded gray
///    circle.
/// 3. A selected conversation, or an ambient (Oz) terminal: the Oz agent variant.
/// 4. Everything else: `None`.
pub(crate) fn agent_icon_variant_from_terminal_inputs(
    inputs: &TerminalIconInputs,
) -> Option<IconWithStatusVariant> {
    if let Some(session) = inputs
        .cli_session
        .as_ref()
        .filter(|s| !matches!(s.agent, CLIAgent::Unknown))
    {
        let status =
            (session.has_listener && session.supports_rich_status).then(|| session.status.clone());
        return Some(IconWithStatusVariant::CLIAgent {
            agent: session.agent,
            status,
            is_ambient: inputs.is_ambient,
        });
    }

    if inputs.is_ambient
        && let Some(agent) = inputs
            .selected_third_party_cli_agent
            .filter(|agent| !matches!(agent, CLIAgent::Unknown))
    {
        return Some(IconWithStatusVariant::CLIAgent {
            agent,
            status: inputs.selected_conversation_status.clone(),
            is_ambient: true,
        });
    }

    if inputs.has_selected_conversation || inputs.is_ambient {
        return Some(IconWithStatusVariant::OzAgent {
            status: inputs.selected_conversation_status.clone(),
            is_ambient: inputs.is_ambient,
        });
    }

    None
}

/// Pure run-card logic: maps a [`Harness`], status, and ambient flag into an
/// [`IconWithStatusVariant`]. Falls back to the Oz variant for [`Harness::Oz`] and
/// [`Harness::Unknown`], the latter so a future harness this client doesn't recognize doesn't
/// render an unbranded gray circle.
pub(crate) fn agent_icon_variant_for_run(
    harness: Harness,
    status: ConversationStatus,
    is_ambient: bool,
) -> IconWithStatusVariant {
    match CLIAgent::from_harness(harness) {
        Some(agent) => IconWithStatusVariant::CLIAgent {
            agent,
            status: Some(status),
            is_ambient,
        },
        None => IconWithStatusVariant::OzAgent {
            status: Some(status),
            is_ambient,
        },
    }
}

/// Run-card variant for a normalized conversation-list entry.
///
/// Deviation from the pinned oracle: the pin's `AgentConversationEntry` carries a separate
/// `provenance` (e.g. `AmbientRun`) and `backing.has_ambient_run` that `is_cloud_agent_run()`
/// consults. This fork's `AgentConversationEntry` (`ai::conversation_entry`) is a deliberately
/// minimized BYOP-local projection -- see that module's doc comment -- with no cloud/ambient
/// run concept at all, so `is_cloud_agent_run()` is hard-coded `false` there. Calling it here
/// is still correct (conversation-list entries in this fork are always local), it just can
/// never return `true` today.
pub(crate) fn agent_conversation_entry_icon_variant(
    entry: &AgentConversationEntry,
) -> IconWithStatusVariant {
    let status = entry.display.status.to_conversation_status();
    agent_icon_variant_for_run(
        entry.display.harness.unwrap_or(Harness::Oz),
        status,
        entry.is_cloud_agent_run(),
    )
}

#[cfg(test)]
#[path = "agent_icon_tests.rs"]
mod tests;
