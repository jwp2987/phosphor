//! Tests for the bounded MCP tool call (#1064), ported from upstream warpdotdev/warp
//! `d5d23e611:crates/mcp/src/tool_call_tests.rs`. The deadline is injected as a future,
//! so these run in real time without waiting for [`super::TOOL_CALL_TIMEOUT`].
//!
//! Not ported: upstream's `ended_sse_without_result_fails_without_waiting_for_deadline`
//! asserts rmcp 3.x behaviour (an SSE response that ends without a result fails with
//! `TransportClosed`); rmcp 2.2 leaves that call pending instead, which is what
//! `ended_sse_without_result_is_bounded_by_the_deadline` covers here.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use futures::future::{pending, ready};
use rmcp::model::{CallToolRequestParams, ErrorData};
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::{ServiceError, ServiceExt};
use serde_json::{Value, json};
use tokio::sync::{Notify, oneshot};
use tokio::task::JoinHandle;
use uuid::Uuid;

use super::{TOOL_CALL_TIMEOUT, call_tool_with_deadline_inner};

#[derive(Clone, Copy)]
enum Reply {
    Success,
    SseSuccess,
    ToolError,
    ProtocolError,
    HttpError,
    WrongId,
    EmptySse,
    IncompleteSse,
    StuckSend,
}

struct ServerState {
    reply: Reply,
    calls: AtomicUsize,
    cancellations: AtomicUsize,
    request_seen: Notify,
    request_id: Mutex<Option<Value>>,
}

struct TestServer {
    uri: String,
    state: Arc<ServerState>,
    task: JoinHandle<()>,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn start_server(reply: Reply) -> TestServer {
    let state = Arc::new(ServerState {
        reply,
        calls: AtomicUsize::new(0),
        cancellations: AtomicUsize::new(0),
        request_seen: Notify::new(),
        request_id: Mutex::new(None),
    });
    let router = Router::new()
        .route(
            "/mcp",
            post(handle_post)
                .get(|| async { StatusCode::METHOD_NOT_ALLOWED })
                .delete(|| async { StatusCode::OK }),
        )
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let uri = format!("http://{}/mcp", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    TestServer { uri, state, task }
}

fn result(id: Value, is_error: bool) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {"content": [], "isError": is_error}
    })
}

fn sse(body: String) -> Response {
    ([("content-type", "text/event-stream")], Body::from(body)).into_response()
}

fn progress(request: &Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "notifications/progress",
        "params": {
            "progressToken": request["params"]["_meta"]["progressToken"],
            "progress": 1
        }
    })
}

async fn handle_post(
    State(state): State<Arc<ServerState>>,
    Json(request): Json<Value>,
) -> Response {
    match request["method"].as_str().unwrap() {
        "initialize" => Json(json!({
            "jsonrpc": "2.0",
            "id": request["id"],
            "result": {
                "protocolVersion": "2025-03-26",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "deadline-test", "version": "1"}
            }
        }))
        .into_response(),
        "notifications/initialized" => StatusCode::ACCEPTED.into_response(),
        "notifications/cancelled" => {
            state.cancellations.fetch_add(1, Ordering::SeqCst);
            StatusCode::ACCEPTED.into_response()
        }
        "tools/call" => {
            state.calls.fetch_add(1, Ordering::SeqCst);
            *state.request_id.lock().unwrap() = Some(request["id"].clone());
            state.request_seen.notify_one();
            if request["params"]["name"] == "succeed" {
                return Json(result(request["id"].clone(), false)).into_response();
            }
            match state.reply {
                Reply::Success => Json(result(request["id"].clone(), false)).into_response(),
                Reply::SseSuccess => sse(format!(
                    "data: {}\n\n",
                    result(request["id"].clone(), false)
                )),
                Reply::ToolError => Json(result(request["id"].clone(), true)).into_response(),
                Reply::ProtocolError => Json(json!({
                    "jsonrpc": "2.0",
                    "id": request["id"],
                    "error": {"code": -32602, "message": "test protocol error"}
                }))
                .into_response(),
                Reply::HttpError => StatusCode::BAD_GATEWAY.into_response(),
                // A server that never answers this call: the only response it sends is
                // for some other request.
                Reply::WrongId => Json(result(json!("unrelated"), false)).into_response(),
                Reply::EmptySse => sse(String::new()),
                Reply::IncompleteSse => sse(format!(
                    "data: {}\n\nevent: message\ndata: {{\"jsonrpc\":\"2.0\"",
                    progress(&request),
                )),
                // A server that never finishes the HTTP request carrying the call.
                Reply::StuckSend => pending().await,
            }
        }
        _ => StatusCode::BAD_REQUEST.into_response(),
    }
}

#[tokio::test]
async fn deadline_includes_connecting_without_dispatch() {
    let result = call_tool_with_deadline_inner(
        Uuid::nil(),
        Uuid::nil(),
        CallToolRequestParams::new("test"),
        pending(),
        TOOL_CALL_TIMEOUT,
        ready(()),
        || ready(()),
    )
    .await;
    let message = result.unwrap_err().to_string();
    assert!(message.contains("not dispatched"), "{message}");
    assert!(message.contains("1800 seconds"), "{message}");
}

#[tokio::test]
async fn connection_error_is_preserved() {
    let result = call_tool_with_deadline_inner(
        Uuid::nil(),
        Uuid::nil(),
        CallToolRequestParams::new("test"),
        ready(Err(ServiceError::McpError(ErrorData::internal_error(
            "test connection error",
            None,
        )))),
        TOOL_CALL_TIMEOUT,
        pending(),
        || ready(()),
    )
    .await;
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("test connection error")
    );
}

#[tokio::test]
async fn successful_and_error_results_complete_without_retry() {
    for reply in [
        Reply::Success,
        Reply::SseSuccess,
        Reply::ToolError,
        Reply::ProtocolError,
        Reply::HttpError,
    ] {
        let server = start_server(reply).await;
        let service =
            ().serve(StreamableHttpClientTransport::from_uri(server.uri.clone()))
                .await
                .unwrap();
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            call_tool_with_deadline_inner(
                Uuid::nil(),
                Uuid::nil(),
                CallToolRequestParams::new("test"),
                ready(Ok(service.peer().clone())),
                TOOL_CALL_TIMEOUT,
                pending(),
                || ready(()),
            ),
        )
        .await
        .unwrap();
        match reply {
            Reply::Success | Reply::SseSuccess => {
                assert_eq!(result.unwrap().is_error, Some(false))
            }
            Reply::ToolError => assert_eq!(result.unwrap().is_error, Some(true)),
            Reply::ProtocolError => assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("test protocol error")
            ),
            Reply::HttpError => assert!(result.is_err()),
            _ => unreachable!(),
        }
        assert_eq!(server.state.calls.load(Ordering::SeqCst), 1);
        assert_eq!(server.state.cancellations.load(Ordering::SeqCst), 0);
        service.cancel().await.unwrap();
    }
}

/// The #1064 repro: a call the server never answers ends in exactly one error result
/// once the deadline passes, and the server is told to cancel it.
#[tokio::test]
async fn unanswered_call_returns_outcome_unknown_at_deadline() {
    let server = start_server(Reply::WrongId).await;
    let service =
        ().serve(StreamableHttpClientTransport::from_uri(server.uri.clone()))
            .await
            .unwrap();
    let (expire, deadline) = oneshot::channel();
    let task = tokio::spawn(call_tool_with_deadline_inner(
        Uuid::nil(),
        Uuid::nil(),
        CallToolRequestParams::new("test"),
        ready(Ok(service.peer().clone())),
        TOOL_CALL_TIMEOUT,
        async { deadline.await.unwrap() },
        pending,
    ));
    server.state.request_seen.notified().await;
    expire.send(()).unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
    let message = result.unwrap_err().to_string();
    assert!(message.contains("outcome is unknown"), "{message}");
    assert!(message.contains("Do not retry"), "{message}");
    assert_eq!(server.state.calls.load(Ordering::SeqCst), 1);
    assert_eq!(server.state.cancellations.load(Ordering::SeqCst), 1);
    service.cancel().await.unwrap();
}

/// rmcp 2.2 leaves a call pending when its SSE response ends without a result; the
/// deadline must still end it, without replaying the tool.
#[tokio::test]
async fn ended_sse_without_result_is_bounded_by_the_deadline() {
    for reply in [Reply::EmptySse, Reply::IncompleteSse] {
        let server = start_server(reply).await;
        let service =
            ().serve(StreamableHttpClientTransport::from_uri(server.uri.clone()))
                .await
                .unwrap();
        let (expire, deadline) = oneshot::channel::<()>();
        let task = tokio::spawn(call_tool_with_deadline_inner(
            Uuid::nil(),
            Uuid::nil(),
            CallToolRequestParams::new("test"),
            ready(Ok(service.peer().clone())),
            TOOL_CALL_TIMEOUT,
            async {
                let _ = deadline.await;
            },
            || ready(()),
        ));
        server.state.request_seen.notified().await;
        // Either rmcp fails the call on its own or the deadline does; it must not hang.
        let _ = expire.send(());
        let result = tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .expect("an ended response must not leave the tool call pending")
            .unwrap();
        assert!(result.is_err());
        assert_eq!(server.state.calls.load(Ordering::SeqCst), 1);
        service.cancel().await.unwrap();
    }
}

#[tokio::test]
async fn timed_out_request_does_not_close_the_connection_or_replay_the_tool() {
    let server = start_server(Reply::WrongId).await;
    let service =
        ().serve(StreamableHttpClientTransport::from_uri(server.uri.clone()))
            .await
            .unwrap();
    let peer = service.peer().clone();
    let (expire, deadline) = oneshot::channel();
    let task = tokio::spawn(call_tool_with_deadline_inner(
        Uuid::nil(),
        Uuid::nil(),
        CallToolRequestParams::new("test"),
        ready(Ok(peer.clone())),
        TOOL_CALL_TIMEOUT,
        async { deadline.await.unwrap() },
        pending,
    ));
    server.state.request_seen.notified().await;
    let succeeding = call_tool_with_deadline_inner(
        Uuid::nil(),
        Uuid::nil(),
        CallToolRequestParams::new("succeed"),
        ready(Ok(peer)),
        TOOL_CALL_TIMEOUT,
        pending(),
        pending,
    );
    assert!(
        tokio::time::timeout(Duration::from_secs(5), succeeding)
            .await
            .unwrap()
            .is_ok()
    );
    expire.send(()).unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    assert_eq!(server.state.calls.load(Ordering::SeqCst), 2);
    assert_eq!(server.state.cancellations.load(Ordering::SeqCst), 1);
    assert!(!service.is_transport_closed());
    service.cancel().await.unwrap();
}

#[tokio::test]
async fn stalled_send_cannot_block_deadline_or_cancellation() {
    let server = start_server(Reply::StuckSend).await;
    let service =
        ().serve(StreamableHttpClientTransport::from_uri(server.uri.clone()))
            .await
            .unwrap();
    let peer = service.peer().clone();
    let (expire, deadline) = oneshot::channel();
    let task = tokio::spawn(call_tool_with_deadline_inner(
        Uuid::nil(),
        Uuid::nil(),
        CallToolRequestParams::new("test"),
        ready(Ok(peer)),
        TOOL_CALL_TIMEOUT,
        async { deadline.await.unwrap() },
        || ready(()),
    ));
    server.state.request_seen.notified().await;
    expire.send(()).unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("outcome is unknown")
    );
    assert_eq!(server.state.calls.load(Ordering::SeqCst), 1);
    assert_eq!(server.state.cancellations.load(Ordering::SeqCst), 0);
    service.cancellation_token().cancel();
}
