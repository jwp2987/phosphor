use std::time::Duration;

use futures::StreamExt;
use futures::executor::block_on;
use agent_proto as api;

use super::{Event, coalesce_append_events};

fn append(message_id: &str, text: &str, reasoning: bool) -> Event {
    let (inner, path) = if reasoning {
        (
            api::message::Message::AgentReasoning(api::message::AgentReasoning {
                reasoning: text.to_owned(),
                finished_duration: None,
            }),
            "agent_reasoning.reasoning",
        )
    } else {
        (
            api::message::Message::AgentOutput(api::message::AgentOutput {
                text: text.to_owned(),
            }),
            "agent_output.text",
        )
    };
    Ok(api::ResponseEvent {
        r#type: Some(api::response_event::Type::ClientActions(
            api::response_event::ClientActions {
                actions: vec![api::ClientAction {
                    action: Some(api::client_action::Action::AppendToMessageContent(
                        api::client_action::AppendToMessageContent {
                            task_id: "t".to_owned(),
                            message: Some(api::Message {
                                id: message_id.to_owned(),
                                task_id: "t".to_owned(),
                                message: Some(inner),
                                ..Default::default()
                            }),
                            mask: Some(prost_types::FieldMask {
                                paths: vec![path.to_owned()],
                            }),
                        },
                    )),
                }],
            },
        )),
    })
}

/// A non-append event (an empty `ClientActions` stands in for any other event).
fn other() -> Event {
    Ok(api::ResponseEvent {
        r#type: Some(api::response_event::Type::ClientActions(
            api::response_event::ClientActions { actions: vec![] },
        )),
    })
}

/// (message id, text) of an append event, or None for anything else.
fn describe(event: &Event) -> Option<(String, String)> {
    let Ok(api::ResponseEvent {
        r#type: Some(api::response_event::Type::ClientActions(actions)),
    }) = event
    else {
        return None;
    };
    let [action] = actions.actions.as_slice() else {
        return None;
    };
    let Some(api::client_action::Action::AppendToMessageContent(append)) = &action.action else {
        return None;
    };
    let message = append.message.as_ref()?;
    let text = match message.message.as_ref()? {
        api::message::Message::AgentOutput(o) => o.text.clone(),
        api::message::Message::AgentReasoning(r) => r.reasoning.clone(),
        _ => return None,
    };
    Some((message.id.clone(), text))
}

fn collect(items: Vec<Event>, window: Duration) -> Vec<Event> {
    block_on(coalesce_append_events(futures::stream::iter(items), window).collect())
}

const LONG: Duration = Duration::from_secs(30);

#[test]
fn first_chunk_is_immediate_and_the_burst_behind_it_is_merged() {
    let out = collect(
        vec![
            append("m", "a", false),
            append("m", "b", false),
            append("m", "c", false),
            append("m", "d", false),
        ],
        LONG,
    );
    let described: Vec<_> = out.iter().map(describe).collect();
    assert_eq!(
        described,
        vec![
            Some(("m".to_owned(), "a".to_owned())),
            Some(("m".to_owned(), "bcd".to_owned())),
        ]
    );
}

#[test]
fn text_is_concatenated_in_order_and_nothing_is_lost() {
    let words: Vec<String> = (0..200).map(|i| format!("w{i} ")).collect();
    let out = collect(words.iter().map(|w| append("m", w, false)).collect(), LONG);
    let joined: String = out
        .iter()
        .filter_map(describe)
        .map(|(_, text)| text)
        .collect();
    assert_eq!(joined, words.concat());
    assert!(
        out.len() <= 2,
        "a ready burst collapses to <=2 events, got {}",
        out.len()
    );
}

#[test]
fn other_events_flush_the_held_chunk_first_and_keep_order() {
    let out = collect(
        vec![
            append("m", "a", false),
            append("m", "b", false),
            other(),
            append("m", "c", false),
        ],
        LONG,
    );
    let described: Vec<_> = out.iter().map(describe).collect();
    assert_eq!(
        described,
        vec![
            Some(("m".to_owned(), "a".to_owned())),
            Some(("m".to_owned(), "b".to_owned())),
            None,
            // `c` follows a flush inside the window, so it is held and emitted at stream end.
            Some(("m".to_owned(), "c".to_owned())),
        ]
    );
}

#[test]
fn different_messages_and_fields_are_never_merged() {
    let out = collect(
        vec![
            append("m1", "a", false),
            append("m1", "b", false),
            append("m2", "c", false),
            append("m2", "r1", true),
            append("m2", "r2", true),
        ],
        LONG,
    );
    let described: Vec<_> = out.iter().filter_map(describe).collect();
    assert_eq!(
        described,
        vec![
            ("m1".to_owned(), "a".to_owned()),
            ("m1".to_owned(), "b".to_owned()),
            ("m2".to_owned(), "c".to_owned()),
            ("m2".to_owned(), "r1r2".to_owned()),
        ]
    );
}

#[test]
fn errors_flush_held_text_before_the_error() {
    let items = vec![
        append("m", "a", false),
        append("m", "b", false),
        Err(std::sync::Arc::new(
            crate::ai::api_error::AIApiError::Other(anyhow::anyhow!("boom")),
        )),
    ];
    let out = collect(items, LONG);
    assert_eq!(out.len(), 3);
    assert_eq!(describe(&out[1]), Some(("m".to_owned(), "b".to_owned())));
    assert!(out[2].is_err());
}

/// A held chunk must be flushed by the timer even if the provider then goes silent.
#[test]
fn held_chunk_is_flushed_by_the_timer_when_the_stream_stalls() {
    let items: Vec<Event> = vec![append("m", "a", false), append("m", "b", false)];
    let stalled = futures::stream::iter(items).chain(futures::stream::pending());
    let mut out = coalesce_append_events(stalled, Duration::from_millis(20));
    let first = block_on(out.next()).expect("first");
    let second = block_on(out.next()).expect("second, released by the timer");
    assert_eq!(describe(&first), Some(("m".to_owned(), "a".to_owned())));
    assert_eq!(describe(&second), Some(("m".to_owned(), "b".to_owned())));
}
