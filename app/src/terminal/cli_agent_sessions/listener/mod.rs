use warpui::{EntityId, ModelContext, ModelHandle, SingletonEntity};

use super::{CLIAgentEvent, CLIAgentSession, CLIAgentSessionsModel};
use crate::features::FeatureFlag;
use crate::terminal::cli_agent_sessions::event::parse_event;
use crate::terminal::cli_agent_sessions::event::{
    CLIAgentEventPayload, CLIAgentEventSource, CLIAgentEventType,
};
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
    ///
    /// `plugin_already_active` is true when the session has already received a
    /// structured OSC 777 notification; Codex uses it to drop OSC 9 fallback
    /// once the rich plugin is active. Other handlers ignore it.
    fn try_parse(
        &mut self,
        title: Option<&str>,
        body: &str,
        plugin_already_active: bool,
    ) -> Option<CLIAgentEvent> {
        let _ = plugin_already_active;
        parse_event(title, body)
    }

    /// Decide whether a parsed event should be forwarded to the sessions model.
    /// Returns the event (possibly transformed) if it should be processed.
    fn handle_event(&mut self, event: CLIAgentEvent) -> Option<CLIAgentEvent>;
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
        // Hermes, Vibe and this fork's own TUI don't emit the structured OSC 777
        // events this listener parses, and have no known plugin/hook integration.
        CLIAgent::Amp
        | CLIAgent::Copilot
        | CLIAgent::CursorCli
        | CLIAgent::Goose
        | CLIAgent::Hermes
        | CLIAgent::Vibe
        | CLIAgent::PhosphorTui
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
            source: CLIAgentEventSource::CodexOsc9Fallback,
        })
    }
}

impl CLIAgentSessionHandler for CodexSessionHandler {
    /// Before Codex had structured plugin support, we relied on OSC 9 to
    /// trigger notifications. Here we try to parse an OSC 777 event first, and
    /// once the session has seen one (`plugin_already_active`), we ignore OSC 9
    /// notifications so we don't double-process both channels.
    fn try_parse(
        &mut self,
        title: Option<&str>,
        body: &str,
        plugin_already_active: bool,
    ) -> Option<CLIAgentEvent> {
        // If the notification carries the structured sentinel, try the normal
        // JSON parser first (future-proofing in case Codex adds plugin
        // support later). A structured event that belongs to a different agent
        // is dropped rather than applied to this Codex session, and must not
        // fall through to the OSC 9 plain-text path either. When the Codex
        // plugin feature flag is off, Codex is expected to speak plain OSC 9
        // only, so a structured event is dropped rather than trusted.
        if let Some(parsed) = parse_event(title, body) {
            if parsed.agent == CLIAgent::Codex {
                if !FeatureFlag::CodexPlugin.is_enabled() {
                    return None;
                }
                return Some(parsed);
            }
            return None;
        }
        // OSC 9 notifications have no title. Skip OSC 9 once the rich plugin is
        // active, otherwise we'd process both OSC 777 and OSC 9 notifications.
        if title.is_some() || plugin_already_active {
            return None;
        }
        Self::parse_osc9_text(body)
    }

    fn handle_event(&mut self, event: CLIAgentEvent) -> Option<CLIAgentEvent> {
        Some(event)
    }
}

/// DeepSeek-TUI handler: listens for structured OSC 777 events and legacy
/// OSC 9 plain-text notifications.
/// DeepSeek-TUI emits `\x1b]9;deepseek: turn complete\x07` (optionally with
/// elapsed time and cost) when a turn finishes. Those legacy notifications are
/// treated as `Stop` events tagged `CodexOsc9Fallback`, so they never latch
/// `CLIAgentSession::received_rich_notification`. Rich status is only latched
/// when DeepSeek hooks emit structured OSC 777 events.
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
    fn try_parse(
        &mut self,
        title: Option<&str>,
        body: &str,
        _plugin_already_active: bool,
    ) -> Option<CLIAgentEvent> {
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
            // Fork-original DeepSeek OSC 9 path. The oracle only has the Codex one,
            // but this is the same category -- a bare text notification with no
            // structured payload -- so it must not be labelled `RichPlugin`.
            source: CLIAgentEventSource::CodexOsc9Fallback,
        })
    }

    fn handle_event(&mut self, event: CLIAgentEvent) -> Option<CLIAgentEvent> {
        Some(event)
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
                let plugin_already_active = CLIAgentSessionsModel::as_ref(ctx)
                    .session(me.terminal_view_id)
                    .is_some_and(|session| session.received_rich_notification);
                let Some(parsed) =
                    me.inner
                        .try_parse(title.as_deref(), body, plugin_already_active)
                else {
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
