use super::*;
// Imported explicitly rather than inherited through `use super::*`: the parent
// module does not reference `CLIAgentSession` itself, so carrying it there only
// to satisfy this glob reads as an unused import.
use crate::terminal::cli_agent_sessions::CLIAgentSession;
use crate::features::FeatureFlag;
use crate::terminal::cli_agent_sessions::event::{
    CLIAgentEventSource, CLIAgentEventType, CLI_AGENT_NOTIFICATION_SENTINEL,
};

#[test]
fn codex_parses_any_text_as_stop() {
    let event = CodexSessionHandler::parse_osc9_text("Agent turn complete").unwrap();
    assert_eq!(event.event, CLIAgentEventType::Stop);
    assert_eq!(event.agent, CLIAgent::Codex);
    assert_eq!(event.payload.query.as_deref(), Some("Agent turn complete"));
}

#[test]
fn codex_body_becomes_query() {
    let event =
        CodexSessionHandler::parse_osc9_text("I've updated the README with the new instructions.")
            .unwrap();
    assert_eq!(event.event, CLIAgentEventType::Stop);
    assert_eq!(
        event.payload.query.as_deref(),
        Some("I've updated the README with the new instructions.")
    );
}

#[test]
fn codex_approval_text_still_becomes_stop() {
    let event =
        CodexSessionHandler::parse_osc9_text("Approval requested: rm -rf /tmp/foo").unwrap();
    assert_eq!(event.event, CLIAgentEventType::Stop);
    assert_eq!(
        event.payload.query.as_deref(),
        Some("Approval requested: rm -rf /tmp/foo")
    );
}

#[test]
fn codex_ignores_empty_body() {
    assert!(CodexSessionHandler::parse_osc9_text("").is_none());
    assert!(CodexSessionHandler::parse_osc9_text("   ").is_none());
}

#[test]
fn codex_try_parse_ignores_titled_notifications() {
    let mut handler = CodexSessionHandler;
    assert!(handler
        .try_parse(Some("some-title"), "Agent turn complete", false)
        .is_none());
}

#[test]
fn codex_try_parse_handles_osc9() {
    let mut handler = CodexSessionHandler;
    let event = handler
        .try_parse(None, "Agent turn complete", false)
        .unwrap();
    assert_eq!(event.event, CLIAgentEventType::Stop);
}

#[test]
fn codex_try_parse_ignores_other_structured_agents() {
    // A structured OSC 777 event belonging to another agent must not be applied
    // to this Codex session, and must not fall through to the OSC 9 plain-text
    // path either. Refs #271.
    let mut handler = CodexSessionHandler;
    let body = r#"{"v":1,"agent":"claude","event":"stop"}"#;

    assert!(handler
        .try_parse(Some(CLI_AGENT_NOTIFICATION_SENTINEL), body, false)
        .is_none());
    assert!(handler
        .try_parse(None, "Agent turn complete", false)
        .is_some());
}

#[test]
fn codex_try_parse_ignores_osc9_when_plugin_already_active() {
    let _guard = FeatureFlag::CodexPlugin.override_enabled(true);
    let mut handler = CodexSessionHandler;
    let body = r#"{"v":1,"agent":"codex","event":"permission_request","summary":"Approve?","tool_name":"Bash"}"#;

    let event = handler
        .try_parse(Some(CLI_AGENT_NOTIFICATION_SENTINEL), body, false)
        .unwrap();

    assert_eq!(event.event, CLIAgentEventType::PermissionRequest);
    // Once the session is rich, OSC 9 fallback is dropped.
    assert!(handler
        .try_parse(None, "Agent turn complete", true)
        .is_none());
}

#[test]
fn codex_try_parse_ignores_structured_event_without_codex_plugin() {
    // Ported from the pin (`02b53fcd8`,
    // `app/src/terminal/cli_agent_sessions/listener/mod_tests.rs`). Pins a real
    // fix: `try_parse` used to return the parsed structured event unconditionally,
    // without consulting `FeatureFlag::CodexPlugin` the way `plugin_manager/codex.rs`
    // does everywhere else. With the plugin flag off, Codex is only supposed to
    // speak plain OSC 9 (see the flag's doc comment in `crates/warp_features`), so a
    // structured OSC 777 event must be dropped -- and, crucially, must NOT fall
    // through to the OSC 9 plain-text path either, since the event did carry the
    // structured sentinel.
    let _guard = FeatureFlag::CodexPlugin.override_enabled(false);
    let mut handler = CodexSessionHandler;
    let body = r#"{"v":1,"agent":"codex","event":"permission_request","summary":"Approve?","tool_name":"Bash"}"#;

    assert!(handler
        .try_parse(Some(CLI_AGENT_NOTIFICATION_SENTINEL), body, false)
        .is_none());
    assert!(handler
        .try_parse(None, "Agent turn complete", false)
        .is_some());
}

#[test]
fn auggie_is_supported() {
    assert!(is_agent_supported(&CLIAgent::Auggie));
}

#[test]
fn auggie_default_handler_skips_session_start() {
    let mut handler = DefaultSessionListener;
    let event = CLIAgentEvent {
        v: 1,
        agent: CLIAgent::Auggie,
        event: CLIAgentEventType::SessionStart,
        session_id: None,
        cwd: None,
        project: None,
        payload: CLIAgentEventPayload::default(),
        source: CLIAgentEventSource::RichPlugin,
    };
    assert!(handler.handle_event(event).is_none());
}

#[test]
fn auggie_default_handler_forwards_stop() {
    let mut handler = DefaultSessionListener;
    let event = CLIAgentEvent {
        v: 1,
        agent: CLIAgent::Auggie,
        event: CLIAgentEventType::Stop,
        session_id: None,
        cwd: None,
        project: None,
        payload: CLIAgentEventPayload::default(),
        source: CLIAgentEventSource::RichPlugin,
    };
    assert!(handler.handle_event(event).is_some());
}

// Ported from warp/master `app/src/terminal/cli_agent_sessions/listener/mod_tests.rs`
// (`droid_is_supported`, `droid_default_handler_skips_session_start`,
// `droid_default_handler_forwards_stop`,
// `droid_default_handler_forwards_permission_request`). Adapted only for the
// fork's `CLIAgentEvent`, which has no `source` field; assertions unchanged.

#[test]
fn droid_is_supported() {
    assert!(is_agent_supported(&CLIAgent::Droid));
}

#[test]
fn droid_default_handler_skips_session_start() {
    let mut handler = DefaultSessionListener;
    let event = CLIAgentEvent {
        v: 1,
        agent: CLIAgent::Droid,
        event: CLIAgentEventType::SessionStart,
        session_id: None,
        cwd: None,
        project: None,
        payload: CLIAgentEventPayload::default(),
        source: CLIAgentEventSource::RichPlugin,
    };
    assert!(handler.handle_event(event).is_none());
}

#[test]
fn droid_default_handler_forwards_stop() {
    let mut handler = DefaultSessionListener;
    let event = CLIAgentEvent {
        v: 1,
        agent: CLIAgent::Droid,
        event: CLIAgentEventType::Stop,
        session_id: None,
        cwd: None,
        project: None,
        payload: CLIAgentEventPayload::default(),
        source: CLIAgentEventSource::RichPlugin,
    };
    assert!(handler.handle_event(event).is_some());
}

#[test]
fn droid_default_handler_forwards_permission_request() {
    let mut handler = DefaultSessionListener;
    let event = CLIAgentEvent {
        v: 1,
        agent: CLIAgent::Droid,
        event: CLIAgentEventType::PermissionRequest,
        session_id: None,
        cwd: None,
        project: None,
        payload: CLIAgentEventPayload::default(),
        source: CLIAgentEventSource::RichPlugin,
    };
    assert!(handler.handle_event(event).is_some());
}

#[test]
fn pi_is_supported() {
    assert!(is_agent_supported(&CLIAgent::Pi));
}

#[test]
fn pi_default_handler_skips_session_start() {
    let mut handler = DefaultSessionListener;
    let event = CLIAgentEvent {
        v: 1,
        agent: CLIAgent::Pi,
        event: CLIAgentEventType::SessionStart,
        session_id: None,
        cwd: None,
        project: None,
        payload: CLIAgentEventPayload::default(),
        source: CLIAgentEventSource::RichPlugin,
    };
    assert!(handler.handle_event(event).is_none());
}

#[test]
fn pi_default_handler_forwards_stop() {
    let mut handler = DefaultSessionListener;
    let event = CLIAgentEvent {
        v: 1,
        agent: CLIAgent::Pi,
        event: CLIAgentEventType::Stop,
        session_id: None,
        cwd: None,
        project: None,
        payload: CLIAgentEventPayload::default(),
        source: CLIAgentEventSource::RichPlugin,
    };
    assert!(handler.handle_event(event).is_some());
}

#[test]
fn omp_is_supported() {
    // The pin calls this variant `CLIAgent::OhMyPi`; the fork calls it
    // `CLIAgent::Omp`. Same agent, same `"omp"` command prefix. Refs #273.
    assert!(is_agent_supported(&CLIAgent::Omp));
}

#[test]
fn omp_end_to_end_parsing_and_handling() {
    let mut handler = create_handler(&CLIAgent::Omp).expect("should create handler");

    // session_start payload: proves SessionStart is skipped.
    let start_body = r#"{"v":1,"agent":"omp","event":"session_start"}"#;
    let parsed_start = handler
        .try_parse(Some(CLI_AGENT_NOTIFICATION_SENTINEL), start_body, false)
        .expect("should successfully parse session_start payload");
    assert_eq!(parsed_start.agent, CLIAgent::Omp);
    assert_eq!(parsed_start.event, CLIAgentEventType::SessionStart);
    assert!(handler.handle_event(parsed_start).is_none());

    // stop payload: proves Stop forwards with CLIAgent::Omp.
    let stop_body = r#"{"v":1,"agent":"omp","event":"stop"}"#;
    let parsed_stop = handler
        .try_parse(Some(CLI_AGENT_NOTIFICATION_SENTINEL), stop_body, false)
        .expect("should successfully parse stop payload");
    assert_eq!(parsed_stop.agent, CLIAgent::Omp);
    assert_eq!(parsed_stop.event, CLIAgentEventType::Stop);

    let handled_stop = handler
        .handle_event(parsed_stop)
        .expect("should forward stop event");
    assert_eq!(handled_stop.agent, CLIAgent::Omp);
    assert_eq!(handled_stop.event, CLIAgentEventType::Stop);
}

#[test]
fn antigravity_is_supported() {
    assert!(is_agent_supported(&CLIAgent::Antigravity));
}

#[test]
fn antigravity_default_handler_skips_session_start() {
    let mut handler = DefaultSessionListener;
    let event = CLIAgentEvent {
        v: 1,
        agent: CLIAgent::Antigravity,
        event: CLIAgentEventType::SessionStart,
        session_id: None,
        cwd: None,
        project: None,
        payload: CLIAgentEventPayload::default(),
        source: CLIAgentEventSource::RichPlugin,
    };
    assert!(handler.handle_event(event).is_none());
}

#[test]
fn antigravity_default_handler_forwards_stop() {
    let mut handler = DefaultSessionListener;
    let event = CLIAgentEvent {
        v: 1,
        agent: CLIAgent::Antigravity,
        event: CLIAgentEventType::Stop,
        session_id: None,
        cwd: None,
        project: None,
        payload: CLIAgentEventPayload::default(),
        source: CLIAgentEventSource::RichPlugin,
    };
    assert!(handler.handle_event(event).is_some());
}

#[test]
fn deepseek_osc9_completion_does_not_claim_prompt_text() {
    let mut handler = DeepSeekSessionHandler;
    let event = handler
        .try_parse(None, "deepseek: turn complete", false)
        .expect("DeepSeek OSC 9 body should parse");

    assert_eq!(event.event, CLIAgentEventType::Stop);
    assert_eq!(event.payload.query, None);
    assert_eq!(
        event.payload.response.as_deref(),
        Some("deepseek: turn complete")
    );
}

#[test]
fn deepseek_osc9_response_text_becomes_notification_title() {
    let mut handler = DeepSeekSessionHandler;
    let event = handler
        .try_parse(
            None,
            "Latest reply text\ndeepseek: turn complete (1m 15s, $0.01)",
            false,
        )
        .expect("DeepSeek OSC 9 body should parse");

    assert_eq!(event.event, CLIAgentEventType::Stop);
    assert_eq!(event.payload.query.as_deref(), Some("Latest reply text"));
    assert_eq!(
        event.payload.response.as_deref(),
        Some("Latest reply text\ndeepseek: turn complete (1m 15s, $0.01)")
    );
}

#[test]
fn deepseek_osc9_plain_response_text_becomes_notification_title() {
    let mut handler = DeepSeekSessionHandler;
    let event = handler
        .try_parse(None, "Latest reply text", false)
        .expect("DeepSeek OSC 9 body should parse");

    assert_eq!(event.event, CLIAgentEventType::Stop);
    assert_eq!(event.payload.query.as_deref(), Some("Latest reply text"));
    assert_eq!(event.payload.response.as_deref(), Some("Latest reply text"));
}

#[test]
fn deepseek_legacy_osc9_session_is_not_rich_status() {
    // Legacy DeepSeek OSC 9 completion notifications are tagged
    // `CodexOsc9Fallback` (see `DeepSeekSessionHandler::try_parse`), so they
    // never latch `received_rich_notification` -- the single source of truth
    // for rich status (#284).
    let session = CLIAgentSession {
        agent: CLIAgent::DeepSeek,
        status: super::super::CLIAgentSessionStatus::InProgress,
        session_context: super::super::CLIAgentSessionContext::default(),
        input_state: super::super::CLIAgentInputState::Closed,
        should_auto_toggle_input: false,
        listener: None,
        remote_host: None,
        plugin_version: None,
        draft_text: None,
        custom_command_prefix: None,
        received_rich_notification: false,
    };

    assert!(!session.supports_rich_status());
}

#[test]
fn deepseek_structured_session_is_rich_status() {
    // A structured OSC 777 DeepSeek hook event (source `RichPlugin`) latches
    // `received_rich_notification` in `CLIAgentSessionsModel::update_from_event`,
    // which also populates `session_id` -- both are set here to reflect that.
    let session = CLIAgentSession {
        agent: CLIAgent::DeepSeek,
        status: super::super::CLIAgentSessionStatus::InProgress,
        session_context: super::super::CLIAgentSessionContext {
            session_id: Some("sess_1234".to_owned()),
            ..Default::default()
        },
        input_state: super::super::CLIAgentInputState::Closed,
        should_auto_toggle_input: false,
        listener: None,
        remote_host: None,
        plugin_version: None,
        draft_text: None,
        custom_command_prefix: None,
        received_rich_notification: true,
    };

    assert!(session.supports_rich_status());
}
