use super::*;
use crate::terminal::cli_agent_sessions::event::{
    CLIAgentEventType, CLI_AGENT_NOTIFICATION_SENTINEL,
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
    let handler = CodexSessionHandler;
    assert!(handler
        .try_parse(Some("some-title"), "Agent turn complete")
        .is_none());
}

#[test]
fn codex_try_parse_handles_osc9() {
    let handler = CodexSessionHandler;
    let event = handler.try_parse(None, "Agent turn complete").unwrap();
    assert_eq!(event.event, CLIAgentEventType::Stop);
}

#[test]
fn codex_try_parse_ignores_other_structured_agents() {
    // A structured OSC 777 event belonging to another agent must not be applied
    // to this Codex session, and must not fall through to the OSC 9 plain-text
    // path either. Refs #271.
    let handler = CodexSessionHandler;
    let body = r#"{"v":1,"agent":"claude","event":"stop"}"#;

    assert!(handler
        .try_parse(Some(CLI_AGENT_NOTIFICATION_SENTINEL), body)
        .is_none());
    assert!(handler.try_parse(None, "Agent turn complete").is_some());
}

#[test]
fn auggie_is_supported() {
    assert!(is_agent_supported(&CLIAgent::Auggie));
}

#[test]
fn auggie_uses_default_handler_with_rich_status() {
    assert!(agent_supports_rich_status(&CLIAgent::Auggie));
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
    };
    assert!(handler.handle_event(event).is_some());
}

#[test]
fn pi_is_supported() {
    assert!(is_agent_supported(&CLIAgent::Pi));
}

#[test]
fn pi_uses_default_handler_with_rich_status() {
    assert!(agent_supports_rich_status(&CLIAgent::Pi));
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
        .try_parse(Some(CLI_AGENT_NOTIFICATION_SENTINEL), start_body)
        .expect("should successfully parse session_start payload");
    assert_eq!(parsed_start.agent, CLIAgent::Omp);
    assert_eq!(parsed_start.event, CLIAgentEventType::SessionStart);
    assert!(handler.handle_event(parsed_start).is_none());

    // stop payload: proves Stop forwards with CLIAgent::Omp.
    let stop_body = r#"{"v":1,"agent":"omp","event":"stop"}"#;
    let parsed_stop = handler
        .try_parse(Some(CLI_AGENT_NOTIFICATION_SENTINEL), stop_body)
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
fn antigravity_uses_default_handler_with_rich_status() {
    assert!(agent_supports_rich_status(&CLIAgent::Antigravity));
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
    };
    assert!(handler.handle_event(event).is_some());
}

#[test]
fn deepseek_handler_supports_structured_rich_status() {
    assert!(agent_supports_rich_status(&CLIAgent::DeepSeek));
}

#[test]
fn deepseek_osc9_completion_does_not_claim_prompt_text() {
    let handler = DeepSeekSessionHandler;
    let event = handler
        .try_parse(None, "deepseek: turn complete")
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
    let handler = DeepSeekSessionHandler;
    let event = handler
        .try_parse(
            None,
            "Latest reply text\ndeepseek: turn complete (1m 15s, $0.01)",
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
    let handler = DeepSeekSessionHandler;
    let event = handler
        .try_parse(None, "Latest reply text")
        .expect("DeepSeek OSC 9 body should parse");

    assert_eq!(event.event, CLIAgentEventType::Stop);
    assert_eq!(event.payload.query.as_deref(), Some("Latest reply text"));
    assert_eq!(event.payload.response.as_deref(), Some("Latest reply text"));
}

#[test]
fn deepseek_legacy_osc9_session_is_not_rich_status() {
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
    };

    assert!(!session_supports_rich_status(&session));
}

#[test]
fn deepseek_structured_session_is_rich_status() {
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
    };

    assert!(session_supports_rich_status(&session));
}
