use warpui::{EntityId, ModelContext, ModelHandle, SingletonEntity};

use super::{CLIAgentEvent, CLIAgentSession, CLIAgentSessionsModel};
use crate::terminal::cli_agent_sessions::event::parse_event;
use crate::terminal::cli_agent_sessions::event::{CLIAgentEventPayload, CLIAgentEventType};
use crate::terminal::model_events::{ModelEvent, ModelEventDispatcher};
use crate::terminal::CLIAgent;

/// Per-agent handler that filters and transforms parsed CLI agent events.
/// Each CLI agent can have a different implementation depending on which events
/// it cares about.
trait CLIAgentSessionHandler {
    /// Attempt to parse a raw `PluggableNotification` into a typed event.
    /// The default implementation delegates to the structured JSON parser
    /// (`parse_event`); agents with non-JSON notification formats (e.g. Codex
    /// OSC 9 plain text) should override this.
    fn try_parse(&self, title: Option<&str>, body: &str) -> Option<CLIAgentEvent> {
        parse_event(title, body)
    }

    /// Decide whether a parsed event should be forwarded to the sessions model.
    /// Returns the event (possibly transformed) if it should be processed.
    fn handle_event(&mut self, event: CLIAgentEvent) -> Option<CLIAgentEvent>;

    /// Whether this handler provides meaningful, fine-grained status
    /// (e.g. in-progress / blocked / success) that should be shown in the UI.
    /// Handlers backed by the structured plugin protocol report rich status;
    /// handlers that only forward opaque OS notifications (e.g. Codex) do not.
    fn supports_rich_status(&self) -> bool {
        true
    }
}

/// Whether the listener for the given agent provides rich status.
/// Returns `false` for agents without a handler or whose handler opts out.
pub fn agent_supports_rich_status(agent: &CLIAgent) -> bool {
    create_handler(agent).is_some_and(|h| h.supports_rich_status())
}

/// Returns whether this concrete session has enough event context to render
/// fine-grained status in UI surfaces.
pub fn session_supports_rich_status(session: &CLIAgentSession) -> bool {
    if !agent_supports_rich_status(&session.agent) {
        return false;
    }

    // DeepSeek has two listener paths:
    // - legacy OSC 9 completion notifications, registered from command detection,
    //   with no session id or lifecycle events;
    // - structured OSC 777 hooks, which include the DeepSeek hook session id.
    // Only the latter can drive rich status accurately.
    if matches!(session.agent, CLIAgent::DeepSeek) && session.session_context.session_id.is_none() {
        return false;
    }

    true
}

/// Returns `true` if the given CLI agent has a supported session handler.
pub fn is_agent_supported(agent: &CLIAgent) -> bool {
    matches!(
        agent,
        CLIAgent::Claude
            | CLIAgent::OpenCode
            | CLIAgent::Codex
            | CLIAgent::Gemini
            | CLIAgent::Auggie
            | CLIAgent::Droid
            | CLIAgent::Pi
            | CLIAgent::DeepSeek
            | CLIAgent::Antigravity
            | CLIAgent::Omp
    )
}

/// Creates the appropriate handler for the given CLI agent.
fn create_handler(agent: &CLIAgent) -> Option<Box<dyn CLIAgentSessionHandler>> {
    match agent {
        // Auggie and Pi are supported via community-maintained plugins
        // (https://github.com/augmentmoogi/auggie-warp,
        // https://github.com/badlogic/pi-mono), which emit the same
        // structured OSC 777 events as the first-party Claude/OpenCode/Gemini
        // plugins. Omp (oh-my-pi) emits these structured OSC 777 events
        // natively. Droid can be supported by user-configured hooks or future
        // integrations that emit the same events. We don't ship install flows
        // for these agents — we just listen.
        CLIAgent::Claude
        | CLIAgent::OpenCode
        | CLIAgent::Gemini
        | CLIAgent::Auggie
        | CLIAgent::Droid
        | CLIAgent::Pi
        | CLIAgent::Omp
        | CLIAgent::Antigravity => Some(Box::new(DefaultSessionListener)),
        CLIAgent::Codex => Some(Box::new(CodexSessionHandler)),
        CLIAgent::DeepSeek => Some(Box::new(DeepSeekSessionHandler)),
        CLIAgent::Amp
        | CLIAgent::Copilot
        | CLIAgent::CursorCli
        | CLIAgent::Goose
        | CLIAgent::Unknown => None,
    }
}

/// Default handler shared by agents whose events need no special filtering
/// beyond skipping the initial `SessionStart`.
struct DefaultSessionListener;

impl CLIAgentSessionHandler for DefaultSessionListener {
    fn handle_event(&mut self, event: CLIAgentEvent) -> Option<CLIAgentEvent> {
        // Skip session_start events (handled during listener construction)
        if event.event == CLIAgentEventType::SessionStart {
            return None;
        }

        Some(event)
    }
}

/// Codex-specific handler that parses plain-text OSC 9 desktop notifications
/// into CLI agent events.
///
/// Codex sends notifications via OSC 9 (`\x1b]9;message\x07`) with
/// human-readable text. Since there's no way to distinguish notification types
/// from the raw text, all OSC 9 notifications are treated as `Stop` (success).
/// The notification body becomes the event's `query` so it surfaces as the
/// notification title in the UI.
struct CodexSessionHandler;

impl CodexSessionHandler {
    /// Parse a plain-text OSC 9 notification body into a `CLIAgentEvent`.
    /// Returns `None` only for empty bodies.
    fn parse_osc9_text(body: &str) -> Option<CLIAgentEvent> {
        let body = body.trim();
        if body.is_empty() {
            return None;
        }

        Some(CLIAgentEvent {
            v: 1,
            agent: CLIAgent::Codex,
            event: CLIAgentEventType::Stop,
            session_id: None,
            cwd: None,
            project: None,
            payload: CLIAgentEventPayload {
                query: Some(body.to_owned()),
                ..Default::default()
            },
        })
    }
}

impl CLIAgentSessionHandler for CodexSessionHandler {
    /// Codex sends plain-text OSC 9 notifications (title = `None`) instead of
    /// the structured OSC 777 JSON used by Claude Code / OpenCode.
    fn try_parse(&self, title: Option<&str>, body: &str) -> Option<CLIAgentEvent> {
        // If the notification carries the structured sentinel, try the normal
        // JSON parser first (future-proofing in case Codex adds plugin
        // support later). A structured event that belongs to a different agent
        // is dropped rather than applied to this Codex session, and must not
        // fall through to the OSC 9 plain-text path either.
        if let Some(parsed) = parse_event(title, body) {
            if parsed.agent == CLIAgent::Codex {
                return Some(parsed);
            }
            return None;
        }
        // OSC 9 notifications have no title.
        if title.is_some() {
            return None;
        }
        Self::parse_osc9_text(body)
    }

    fn handle_event(&mut self, event: CLIAgentEvent) -> Option<CLIAgentEvent> {
        Some(event)
    }

    fn supports_rich_status(&self) -> bool {
        false
    }
}

/// DeepSeek-TUI handler: listens for structured OSC 777 events and legacy
/// OSC 9 plain-text notifications.
/// DeepSeek-TUI emits `\x1b]9;deepseek: turn complete\x07` (optionally with
/// elapsed time and cost) when a turn finishes. Those legacy notifications are
/// treated as `Stop` events. Rich status is only available when DeepSeek hooks
/// emit structured OSC 777 events with a session id.
struct DeepSeekSessionHandler;

impl DeepSeekSessionHandler {
    fn notification_title_from_body(body: &str) -> Option<String> {
        let title = body
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .filter(|line| !line.starts_with("deepseek: turn complete"))
            .collect::<Vec<_>>()
            .join("\n");

        if title.is_empty() {
            None
        } else {
            Some(title)
        }
    }
}

impl CLIAgentSessionHandler for DeepSeekSessionHandler {
    /// DeepSeek-TUI uses OSC 9 with no title (same channel as Codex).
    fn try_parse(&self, title: Option<&str>, body: &str) -> Option<CLIAgentEvent> {
        // Future-proof: try structured JSON first in case a plugin is added later.
        if let Some(parsed) = parse_event(title, body) {
            return Some(parsed);
        }
        // OSC 9 notifications have no title.
        if title.is_some() {
            return None;
        }
        let body = body.trim();
        if body.is_empty() {
            return None;
        }
        Some(CLIAgentEvent {
            v: 1,
            agent: CLIAgent::DeepSeek,
            event: CLIAgentEventType::Stop,
            session_id: None,
            cwd: None,
            project: None,
            payload: CLIAgentEventPayload {
                query: Self::notification_title_from_body(body),
                response: Some(body.to_owned()),
                ..Default::default()
            },
        })
    }

    fn handle_event(&mut self, event: CLIAgentEvent) -> Option<CLIAgentEvent> {
        Some(event)
    }

    fn supports_rich_status(&self) -> bool {
        true
    }
}

/// Per-agent listener that subscribes to PTY events and forwards them to the
/// sessions model. Stored on [`super::CLIAgentSession`] so its lifetime is
/// tied to the session; dropping the handle cleans up the subscription.
pub struct CLIAgentSessionListener {
    terminal_view_id: EntityId,
    inner: Box<dyn CLIAgentSessionHandler>,
}

impl warpui::Entity for CLIAgentSessionListener {
    type Event = ();
}

impl CLIAgentSessionListener {
    pub fn new(
        terminal_view_id: EntityId,
        agent: CLIAgent,
        model_event_dispatcher: &ModelHandle<ModelEventDispatcher>,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        let handler =
            create_handler(&agent).expect("is_agent_supported must be checked before calling new");

        // Subscribe to subsequent OSC events from this terminal's PTY.
        // Parsing is delegated to the handler's `try_parse`; the handler's
        // `handle_event` then filters/transforms the result.
        ctx.subscribe_to_model(model_event_dispatcher, move |me, event, ctx| {
            if let ModelEvent::PluggableNotification { title, body } = event {
                let Some(parsed) = me.inner.try_parse(title.as_deref(), body) else {
                    return;
                };
                if let Some(event) = me.inner.handle_event(parsed) {
                    CLIAgentSessionsModel::handle(ctx).update(ctx, |sessions_model, ctx| {
                        sessions_model.update_from_event(me.terminal_view_id, &event, ctx);
                    });
                }
            }
        });

        Self {
            terminal_view_id,
            inner: handler,
        }
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
