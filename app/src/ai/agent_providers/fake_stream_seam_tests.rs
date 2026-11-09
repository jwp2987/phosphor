use futures::StreamExt;
use genai::chat::{ChatMessage, ChatRequest, ChatStreamEvent, ToolCall, ToolResponse};

use super::*;

fn request_with_tool_round() -> ChatRequest {
    let call = ToolCall {
        call_id: "call-1".to_owned(),
        fn_name: "run_shell_command".to_owned(),
        fn_arguments: Value::String("{}".to_owned()),
        thought_signatures: None,
    };
    ChatRequest::from_messages(vec![
        ChatMessage::user("do it"),
        ChatMessage::from(vec![call]),
        ChatMessage::from(ToolResponse::new("call-1", "done")),
    ])
}

async fn drain(stream: SeamStream) -> Vec<genai::Result<ChatStreamEvent>> {
    stream.collect().await
}

#[test]
fn a_model_id_without_a_script_is_not_intercepted() {
    let request = ChatRequest::from_messages(vec![ChatMessage::user("hi")]);
    assert!(intercept("seam-test-none", &request).is_none());
}

#[test]
fn requests_are_recorded_with_their_tool_messages() {
    let handle = install("seam-test-record", |_| SeamTurn::text("ok"));
    let stream = intercept("seam-test-record", &request_with_tool_round())
        .expect("scripted")
        .expect("opens");
    let events = futures::executor::block_on(drain(stream));
    assert!(matches!(events.first(), Some(Ok(ChatStreamEvent::Start))));
    assert!(matches!(events.last(), Some(Ok(ChatStreamEvent::End(_)))));
    let requests = handle.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].last_user_text(), Some("do it"));
    assert_eq!(requests[0].tool_response_ids(), vec!["call-1"]);
    assert_eq!(requests[0].messages[1].tool_call_ids, vec!["call-1"]);
}

#[test]
fn dropping_the_handle_unregisters_the_model_id() {
    let request = ChatRequest::from_messages(vec![ChatMessage::user("hi")]);
    drop(install("seam-test-drop", |_| SeamTurn::text("ok")));
    assert!(intercept("seam-test-drop", &request).is_none());
}

#[test]
fn tool_calls_stream_as_chunks_and_end_with_parsed_arguments() {
    let _handle = install("seam-test-calls", |_| SeamTurn {
        open_error: None,
        events: vec![
            SeamEvent::Text("hi".to_owned()),
            SeamEvent::ToolCall {
                id: "c1".to_owned(),
                name: "grep".to_owned(),
                args: "{\"q\":1}".to_owned(),
            },
            SeamEvent::ToolCall {
                id: "c2".to_owned(),
                name: "grep".to_owned(),
                args: "{\"q\":".to_owned(),
            },
        ],
        end: true,
    });
    let request = ChatRequest::from_messages(vec![ChatMessage::user("hi")]);
    let stream = intercept("seam-test-calls", &request).unwrap().unwrap();
    let events = futures::executor::block_on(drain(stream));
    let Some(Ok(ChatStreamEvent::End(end))) = events.into_iter().last() else {
        panic!("stream should end with End");
    };
    let calls = end.captured_content.unwrap().into_tool_calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].fn_arguments, serde_json::json!({"q": 1}));
    assert_eq!(calls[1].fn_arguments, Value::String("{\"q\":".to_owned()));
}

#[test]
fn an_open_error_and_a_mid_stream_status_carry_their_status() {
    let _open = install("seam-test-open", |_| SeamTurn {
        open_error: Some(SeamError::Http {
            status: 503,
            body: "overloaded".to_owned(),
        }),
        ..SeamTurn::default()
    });
    let request = ChatRequest::from_messages(vec![ChatMessage::user("hi")]);
    let Err(error) = intercept("seam-test-open", &request).unwrap() else {
        panic!("open should fail");
    };
    assert!(error.to_string().contains("503"), "{error}");
}

#[test]
#[should_panic(expected = "already installed")]
fn installing_one_model_id_twice_panics() {
    let _a = install("seam-test-twice", |_| SeamTurn::text("a"));
    let _b = install("seam-test-twice", |_| SeamTurn::text("b"));
}
