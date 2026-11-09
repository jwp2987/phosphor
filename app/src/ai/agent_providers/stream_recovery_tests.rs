use std::sync::{Arc, Mutex};

use futures::StreamExt;
use genai::chat::{ChatMessage, ChatRequest, ToolCall, ToolResponse};
use serde_json::Value;

use super::super::fake_stream_seam::{
    RecordedRole, SeamError, SeamEvent, SeamTurn, install, intercept,
};
use super::*;

fn call(id: &str) -> ToolCall {
    ToolCall {
        call_id: id.to_owned(),
        fn_name: "grep".to_owned(),
        fn_arguments: Value::String("{}".to_owned()),
        thought_signatures: None,
    }
}

/// system, then two old tool rounds, an old answer, and the latest user message.
fn long_conversation() -> ChatRequest {
    let big = "x".repeat(4_000);
    let mut req = ChatRequest::from_messages(vec![
        ChatMessage::user("first question"),
        ChatMessage::from(vec![call("c1"), call("c2")]),
        ChatMessage::from(ToolResponse::new("c1", big.clone())),
        ChatMessage::from(ToolResponse::new("c2", big.clone())),
        ChatMessage::assistant("first answer"),
        ChatMessage::user("second question"),
        ChatMessage::from(vec![call("c3")]),
        ChatMessage::from(ToolResponse::new("c3", big)),
        ChatMessage::assistant("second answer"),
        ChatMessage::user("latest question"),
    ]);
    req.system = Some("system prompt".to_owned());
    req
}

struct Run {
    result: Result<Opened, OpenFailure>,
    waits: Vec<Duration>,
}

fn run(model: &str, request: ChatRequest, target: Option<usize>) -> Run {
    let waits: Arc<Mutex<Vec<Duration>>> = Arc::default();
    let recorded = waits.clone();
    let model = model.to_owned();
    let result = futures::executor::block_on(open_with_recovery(
        request,
        target,
        |req| {
            let opened = intercept(&model, &req).expect("script installed");
            async move { opened }
        },
        |wait| {
            recorded.lock().unwrap().push(wait);
            async {}
        },
        futures::future::pending::<()>,
    ));
    let waits = waits.lock().unwrap().clone();
    Run { result, waits }
}

fn fail_then_ok(
    first: SeamError,
    failures: usize,
) -> impl FnMut(&super::super::fake_stream_seam::RecordedRequest) -> SeamTurn + Send {
    move |req| {
        if req.index < failures {
            SeamTurn {
                open_error: Some(first.clone()),
                ..SeamTurn::default()
            }
        } else {
            SeamTurn::text("ok")
        }
    }
}

#[test]
fn a_429_with_retry_after_waits_that_long_then_succeeds() {
    let handle = install(
        "recovery-429",
        fail_then_ok(
            SeamError::HttpRetryAfter {
                status: 429,
                body: "slow down".to_owned(),
                retry_after: "7".to_owned(),
            },
            1,
        ),
    );
    let out = run("recovery-429", long_conversation(), None);
    let opened = out.result.ok().expect("second request succeeds");
    assert_eq!(out.waits, vec![Duration::from_secs(7)]);
    assert_eq!(opened.retries, 1);
    assert_eq!(handle.request_count(), 2);
}

#[test]
fn transient_errors_wait_two_four_eight_seconds() {
    let handle = install(
        "recovery-503",
        fail_then_ok(
            SeamError::Http {
                status: 503,
                body: "overloaded".to_owned(),
            },
            3,
        ),
    );
    let out = run("recovery-503", long_conversation(), None);
    assert!(out.result.is_ok());
    assert_eq!(
        out.waits,
        vec![
            Duration::from_secs(2),
            Duration::from_secs(4),
            Duration::from_secs(8)
        ]
    );
    assert_eq!(handle.request_count(), 4);
}

#[test]
fn the_schedule_runs_out_and_the_user_is_told() {
    let handle = install(
        "recovery-exhaust",
        fail_then_ok(
            SeamError::Http {
                status: 500,
                body: "boom".to_owned(),
            },
            usize::MAX,
        ),
    );
    let out = run("recovery-exhaust", long_conversation(), None);
    let failure = out.result.err().expect("gives up");
    assert_eq!(handle.request_count(), 4);
    assert_eq!(failure.waits.len(), 3);
    let note = failure.user_note().expect("a note");
    assert!(note.contains("retried 3 time(s)"), "{note}");
    assert!(note.contains("2s, 4s, 8s"), "{note}");
}

#[test]
fn a_rejected_key_is_not_retried() {
    let handle = install(
        "recovery-401",
        fail_then_ok(
            SeamError::Http {
                status: 401,
                body: "bad key".to_owned(),
            },
            usize::MAX,
        ),
    );
    let out = run("recovery-401", long_conversation(), None);
    assert!(out.result.is_err());
    assert_eq!(handle.request_count(), 1);
    assert!(out.waits.is_empty());
}

#[test]
fn a_drop_before_any_output_is_retried() {
    let handle = install("recovery-early-drop", |req| {
        if req.index == 0 {
            SeamTurn {
                events: vec![SeamEvent::Fail(SeamError::Transport("reset".to_owned()))],
                ..SeamTurn::default()
            }
        } else {
            SeamTurn::text("ok")
        }
    });
    let out = run("recovery-early-drop", long_conversation(), None);
    assert!(out.result.is_ok());
    assert_eq!(handle.request_count(), 2);
    assert_eq!(out.waits, vec![Duration::from_secs(2)]);
}

#[test]
fn a_drop_after_output_is_never_retried() {
    let handle = install("recovery-late-drop", |_| SeamTurn {
        open_error: None,
        events: vec![
            SeamEvent::Text("partial".to_owned()),
            SeamEvent::Fail(SeamError::Transport("connection reset".to_owned())),
        ],
        end: false,
    });
    let out = run("recovery-late-drop", long_conversation(), None);
    let opened = out
        .result
        .ok()
        .expect("output had started, so the stream is handed over");
    assert!(out.waits.is_empty());
    // The loop reads the text, then the error: it is surfaced, not retried.
    let items: Vec<_> = futures::executor::block_on(opened.stream.collect::<Vec<_>>());
    assert!(items.iter().any(|i| i.is_err()));
    assert_eq!(handle.request_count(), 1);
}

const OVERFLOW_BODY: &str = "This model's maximum context length is 8192 tokens. However, your \
                             messages resulted in 9001 tokens.";

#[test]
fn an_overflow_answer_trims_older_turns_and_retries_once() {
    let handle = install("recovery-overflow", |req| {
        if req.index == 0 {
            SeamTurn {
                open_error: Some(SeamError::Http {
                    status: 400,
                    body: OVERFLOW_BODY.to_owned(),
                }),
                ..SeamTurn::default()
            }
        } else {
            SeamTurn::text("ok")
        }
    });
    let out = run("recovery-overflow", long_conversation(), None);
    let opened = out.result.ok().expect("retry after trimming succeeds");
    assert!(opened.compaction.is_some());
    assert!(out.waits.is_empty(), "an overflow is not waited on");
    let requests = handle.requests();
    assert_eq!(requests.len(), 2);
    assert!(requests[1].messages.len() < requests[0].messages.len());
    // The latest user message survives, and every tool result still has its call (I4).
    assert!(
        requests[1]
            .last_user_text()
            .is_some_and(|t| t.ends_with("latest question"))
    );
    let mut calls: Vec<&str> = Vec::new();
    for m in &requests[1].messages {
        calls.extend(m.tool_call_ids.iter().map(String::as_str));
        for id in &m.tool_response_ids {
            assert!(calls.contains(&id.as_str()), "orphaned result {id}");
        }
    }
    assert!(
        requests[1]
            .messages
            .iter()
            .any(|m| m.text.contains("left out"))
    );
}

#[test]
fn a_second_overflow_answer_is_surfaced_not_retried_again() {
    let handle = install("recovery-overflow-twice", |_| SeamTurn {
        open_error: Some(SeamError::Http {
            status: 400,
            body: OVERFLOW_BODY.to_owned(),
        }),
        ..SeamTurn::default()
    });
    let out = run("recovery-overflow-twice", long_conversation(), None);
    let failure = out.result.err().expect("gives up");
    assert!(failure.overflow_unresolved);
    assert_eq!(handle.request_count(), 2);
    let note = failure.user_note().unwrap();
    assert!(note.contains("still too long"), "{note}");
}

#[test]
fn an_overflow_with_nothing_older_to_drop_is_surfaced_at_once() {
    let handle = install("recovery-overflow-short", |_| SeamTurn {
        open_error: Some(SeamError::Http {
            status: 400,
            body: OVERFLOW_BODY.to_owned(),
        }),
        ..SeamTurn::default()
    });
    let short = ChatRequest::from_messages(vec![ChatMessage::user("only message")]);
    let out = run("recovery-overflow-short", short, None);
    let failure = out.result.err().expect("gives up");
    assert!(failure.overflow_unresolved);
    assert_eq!(handle.request_count(), 1);
    assert!(failure.user_note().unwrap().contains("nothing older"));
}

#[test]
fn a_prompt_over_the_target_is_trimmed_before_the_first_send() {
    let handle = install("recovery-proactive", |_| SeamTurn::text("ok"));
    let before = request_bytes(&long_conversation());
    let out = run("recovery-proactive", long_conversation(), Some(before / 2));
    let opened = out.result.ok().expect("opens");
    assert!(opened.compaction.is_some());
    assert_eq!(handle.request_count(), 1);
    assert!(handle.requests()[0].messages.len() < 10);
}

#[test]
fn compaction_keeps_system_the_latest_user_and_whole_tool_rounds() {
    let mut req = long_conversation();
    req.messages
        .insert(0, ChatMessage::system("pinned system message"));
    let compaction = compact_request(&mut req, 1).expect("something to drop");
    assert!(compaction.dropped_messages >= 1);
    assert_eq!(req.messages[0].role, ChatRole::System);
    let last = req.messages.last().unwrap();
    assert!(
        last.content
            .joined_texts()
            .is_some_and(|t| t.ends_with("latest question"))
    );
    assert_eq!(last.role, ChatRole::User);
    assert!(tool_pairs_are_intact(&req));
    // Everything droppable went: a tail of just the latest user message plus the marker.
    let texts: Vec<String> = req
        .messages
        .iter()
        .filter(|m| m.role != ChatRole::System)
        .map(|m| m.content.joined_texts().unwrap_or_default())
        .collect();
    assert!(texts.iter().any(|t| t.contains(COMPACTION_MARKER)));
    assert!(texts.iter().all(|t| !t.contains("first question")));
}

#[test]
fn compaction_never_splits_an_assistant_call_from_its_results() {
    // Target sized so exactly the first unit (user "first question") goes, then stops: the next
    // unit is the call + two results, which must go whole or stay whole.
    for target in [0usize, 1_000, 5_000, 9_000, 12_000, 100_000] {
        let mut req = long_conversation();
        let _ = compact_request(&mut req, target);
        assert!(tool_pairs_are_intact(&req), "target {target}");
        for m in &req.messages {
            if m.content.contains_tool_response() {
                assert_eq!(m.role, ChatRole::Tool);
            }
        }
    }
}

#[test]
fn nothing_is_dropped_without_an_earlier_turn() {
    let mut req = ChatRequest::from_messages(vec![ChatMessage::user("hi")]);
    assert_eq!(compact_request(&mut req, 0), None);
    assert_eq!(req.messages.len(), 1);
}

#[test]
fn classification_follows_the_failure_rules() {
    let status = |code: u16, body: &str, ra: Option<&str>| {
        let inner = genai::Error::HttpError {
            status: http::StatusCode::from_u16(code).unwrap(),
            canonical_reason: String::new(),
            body: body.to_owned(),
            retry_after: ra.map(str::to_owned),
        };
        genai::Error::WebStream {
            model_iden: genai::ModelIden::new(genai::adapter::AdapterKind::OpenAI, "m"),
            cause: inner.to_string(),
            error: Box::new(inner),
        }
    };
    assert_eq!(
        classify(&status(429, "", Some("5"))),
        FailureClass::Retryable {
            retry_after: Some(Duration::from_secs(5))
        }
    );
    assert_eq!(
        classify(&status(429, "", Some("junk"))),
        FailureClass::Retryable { retry_after: None }
    );
    assert_eq!(
        classify(&status(502, "", None)),
        FailureClass::Retryable { retry_after: None }
    );
    assert_eq!(classify(&status(401, "", None)), FailureClass::Fatal);
    assert_eq!(classify(&status(404, "", None)), FailureClass::Fatal);
    assert_eq!(
        classify(&status(400, "prompt is too long: 250000 tokens", None)),
        FailureClass::ContextOverflow
    );
    assert_eq!(
        classify(&status(400, "invalid tool schema", None)),
        FailureClass::Fatal
    );
    assert_eq!(
        classify(&genai::Error::Internal("x".to_owned())),
        FailureClass::Fatal
    );
}

#[test]
fn retry_wait_uses_the_schedule_and_prefers_retry_after() {
    assert_eq!(retry_wait(0, None), Some(Duration::from_secs(2)));
    assert_eq!(retry_wait(2, None), Some(Duration::from_secs(8)));
    assert_eq!(retry_wait(3, None), None);
    assert_eq!(
        retry_wait(1, Some(Duration::from_secs(30))),
        Some(Duration::from_secs(30))
    );
}

#[test]
fn the_recorded_roles_are_what_the_seam_reports() {
    // Guards the assumption the trimming tests above rely on: tool results arrive as Tool role.
    let handle = install("recovery-roles", |_| SeamTurn::text("ok"));
    let _ = run("recovery-roles", long_conversation(), None).result.ok();
    let first = &handle.requests()[0];
    assert!(first.messages.iter().any(|m| m.role == RecordedRole::Tool));
}
