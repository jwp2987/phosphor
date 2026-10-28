//! Tests for the bounded MCP tool call (#1064), ported from upstream warpdotdev/warp
//! `d5d23e611:crates/mcp/src/tool_call_tests.rs`. Most inject the deadline as a future,
//! so they run without waiting for [`super::DEFAULT_TOOL_CALL_TIMEOUT`]; the progress
//! tests use short real timers.
//!
//! Not ported: upstream's `ended_sse_without_result_fails_without_waiting_for_deadline`
//! asserts rmcp 3.x behaviour (an SSE response that ends without a result fails with
//! `TransportClosed`); rmcp 2.2 leaves that call pending instead, which is what
//! `ended_sse_without_result_is_bounded_by_the_deadline` covers here.

use std::num::NonZeroU64;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use futures::future::{BoxFuture, Ready, Shared, pending, ready};
use futures::{FutureExt, StreamExt};
use rmcp::model::{CallToolRequestParams, ClientInfo, ErrorData};
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::{Peer, RoleClient, ServiceError, ServiceExt};
use serde_json::{Value, json};
use tokio::sync::{Notify, oneshot};
use tokio::task::JoinHandle;
use uuid::Uuid;

use super::{
    DEFAULT_TOOL_CALL_TIMEOUT, MAX_TOOL_CALL_TIMEOUT, MIN_TOTAL_TOOL_CALL_TIMEOUT,
    McpClientHandler, ToolCallLimits, call_tool_with_deadline, call_tool_with_deadline_inner,
    tool_call_timeout, total_tool_call_timeout,
};

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
    /// An SSE response that sends a progress notification for the call every
    /// [`PROGRESS_INTERVAL`], [`PROGRESS_EVENTS`] times, and then the result.
    ProgressThenResult,
    /// Like `ProgressThenResult`, but every notification names another progress token
    /// and the result never comes.
    OtherTokenProgress,
    /// An SSE response that stays open and sends nothing.
    SilentSse,
    /// An SSE response that sends a progress notification for the call every
    /// [`PROGRESS_INTERVAL`] and never the result.
    ProgressForever,
    /// The response to `tools/call` breaks off mid-stream, dropping the connection.
    DropOnCall,
}

const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);
const PROGRESS_EVENTS: usize = 50;

/// The limits for an idle timeout, with the production total.
fn limits(idle: Duration) -> ToolCallLimits {
    ToolCallLimits::for_idle_timeout(idle)
}

/// A `connect` that always hands out `peer`.
fn connected(
    peer: Peer<RoleClient>,
) -> impl FnMut() -> Ready<Result<Peer<RoleClient>, ServiceError>> + Send + 'static {
    move || ready(Ok(peer.clone()))
}

/// A deadline the test expires by hand: every idle period the call starts waits on the
/// same signal.
fn manual_deadline() -> (
    oneshot::Sender<()>,
    impl FnMut() -> Shared<BoxFuture<'static, ()>> + Send + 'static,
) {
    let (expire, expired) = oneshot::channel::<()>();
    let expired = async move {
        let _ = expired.await;
    }
    .boxed()
    .shared();
    (expire, move || expired.clone())
}

/// An SSE response body that sends `events` one [`PROGRESS_INTERVAL`] apart, then stays
/// open.
fn sse_events(events: impl Iterator<Item = Value> + Send + 'static) -> Response {
    let events = futures::stream::unfold(events, |mut events| async move {
        let event = events.next()?;
        tokio::time::sleep(PROGRESS_INTERVAL).await;
        Some((
            Ok::<_, std::io::Error>(format!("data: {event}\n\n")),
            events,
        ))
    })
    .chain(futures::stream::pending());
    (
        [("content-type", "text/event-stream")],
        Body::from_stream(events),
    )
        .into_response()
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

fn progress_event(token: Value, progress: usize) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "notifications/progress",
        "params": {"progressToken": token, "progress": progress}
    })
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
                Reply::ProgressThenResult => {
                    let token = request["params"]["_meta"]["progressToken"].clone();
                    let id = request["id"].clone();
                    sse_events(
                        (0..PROGRESS_EVENTS)
                            .map(move |n| progress_event(token.clone(), n))
                            .chain(std::iter::once(result(id, false))),
                    )
                }
                Reply::OtherTokenProgress => {
                    sse_events((0..).map(|n| progress_event(json!("not-this-call"), n)))
                }
                Reply::SilentSse => sse_events(std::iter::empty()),
                Reply::ProgressForever => {
                    let token = request["params"]["_meta"]["progressToken"].clone();
                    sse_events((0..).map(move |n| progress_event(token.clone(), n)))
                }
                Reply::DropOnCall => (
                    [("content-type", "text/event-stream")],
                    Body::from_stream(futures::stream::once(async {
                        Err::<String, _>(std::io::Error::new(
                            std::io::ErrorKind::ConnectionReset,
                            "test drops the connection",
                        ))
                    })),
                )
                    .into_response(),
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
        pending,
        limits(DEFAULT_TOOL_CALL_TIMEOUT),
        || ready(()),
        pending,
        || ready(()),
    )
    .await;
    let message = result.unwrap_err().to_string();
    assert!(message.contains("not dispatched"), "{message}");
    assert!(message.contains("300 seconds"), "{message}");
}

#[tokio::test]
async fn connection_error_is_preserved() {
    let result = call_tool_with_deadline_inner(
        Uuid::nil(),
        Uuid::nil(),
        CallToolRequestParams::new("test"),
        || {
            ready(Err(ServiceError::McpError(ErrorData::internal_error(
                "test connection error",
                None,
            ))))
        },
        limits(DEFAULT_TOOL_CALL_TIMEOUT),
        pending,
        pending,
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
                connected(service.peer().clone()),
                limits(DEFAULT_TOOL_CALL_TIMEOUT),
                pending,
                pending,
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
    let (expire, deadline) = manual_deadline();
    let task = tokio::spawn(call_tool_with_deadline_inner(
        Uuid::nil(),
        Uuid::nil(),
        CallToolRequestParams::new("test"),
        connected(service.peer().clone()),
        limits(DEFAULT_TOOL_CALL_TIMEOUT),
        deadline,
        pending,
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
        let (expire, deadline) = manual_deadline();
        let task = tokio::spawn(call_tool_with_deadline_inner(
            Uuid::nil(),
            Uuid::nil(),
            CallToolRequestParams::new("test"),
            connected(service.peer().clone()),
            limits(DEFAULT_TOOL_CALL_TIMEOUT),
            deadline,
            pending,
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
    let (expire, deadline) = manual_deadline();
    let task = tokio::spawn(call_tool_with_deadline_inner(
        Uuid::nil(),
        Uuid::nil(),
        CallToolRequestParams::new("test"),
        connected(peer.clone()),
        limits(DEFAULT_TOOL_CALL_TIMEOUT),
        deadline,
        pending,
        pending,
    ));
    server.state.request_seen.notified().await;
    let succeeding = call_tool_with_deadline_inner(
        Uuid::nil(),
        Uuid::nil(),
        CallToolRequestParams::new("succeed"),
        connected(peer),
        limits(DEFAULT_TOOL_CALL_TIMEOUT),
        pending,
        pending,
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
    let (expire, deadline) = manual_deadline();
    let task = tokio::spawn(call_tool_with_deadline_inner(
        Uuid::nil(),
        Uuid::nil(),
        CallToolRequestParams::new("test"),
        connected(peer),
        limits(DEFAULT_TOOL_CALL_TIMEOUT),
        deadline,
        pending,
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

/// Connects with the handler production uses, so progress notifications are routed.
async fn connect_with_progress_routing(
    server: &TestServer,
    installation_id: Uuid,
) -> rmcp::service::RunningService<rmcp::RoleClient, McpClientHandler> {
    McpClientHandler::new(ClientInfo::default(), installation_id)
        .serve(StreamableHttpClientTransport::from_uri(server.uri.clone()))
        .await
        .unwrap()
}

/// Progress for the call restarts the idle timeout: the server answers after ~5 s, more
/// than three times the 1.5 s limit, and the call still succeeds.
#[tokio::test]
async fn progress_notifications_reset_the_idle_timeout() {
    let installation_id = Uuid::new_v4();
    let server = start_server(Reply::ProgressThenResult).await;
    let service = connect_with_progress_routing(&server, installation_id).await;
    let timeout = Duration::from_millis(1500);
    let started = std::time::Instant::now();
    let result = tokio::time::timeout(
        Duration::from_secs(30),
        call_tool_with_deadline_inner(
            Uuid::nil(),
            installation_id,
            CallToolRequestParams::new("test"),
            connected(service.peer().clone()),
            limits(timeout),
            move || tokio::time::sleep(timeout),
            pending,
            || ready(()),
        ),
    )
    .await
    .expect("the call ends once the server answers");
    assert_eq!(result.unwrap().is_error, Some(false));
    assert!(started.elapsed() > timeout * 3, "{:?}", started.elapsed());
    assert_eq!(server.state.calls.load(Ordering::SeqCst), 1);
    assert_eq!(server.state.cancellations.load(Ordering::SeqCst), 0);
    // The response stream stays open after the result; don't wait for it to close.
    service.cancellation_token().cancel();
}

/// A server that keeps its response open but says nothing times out after the limit.
#[tokio::test]
async fn silent_server_times_out_after_the_idle_timeout() {
    let installation_id = Uuid::new_v4();
    let server = start_server(Reply::SilentSse).await;
    let service = connect_with_progress_routing(&server, installation_id).await;
    let timeout = Duration::from_millis(300);
    let started = std::time::Instant::now();
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        call_tool_with_deadline_inner(
            Uuid::nil(),
            installation_id,
            CallToolRequestParams::new("test"),
            connected(service.peer().clone()),
            limits(timeout),
            move || tokio::time::sleep(timeout),
            pending,
            pending,
        ),
    )
    .await
    .expect("a silent server must not leave the tool call pending");
    let message = result.unwrap_err().to_string();
    assert!(message.contains("outcome is unknown"), "{message}");
    assert!(message.contains("asked to cancel"), "{message}");
    assert!(message.contains("tool_timeout_secs"), "{message}");
    assert!(started.elapsed() >= timeout);
    assert_eq!(server.state.calls.load(Ordering::SeqCst), 1);
    assert_eq!(server.state.cancellations.load(Ordering::SeqCst), 1);
    service.cancellation_token().cancel();
}

/// Only progress for *this* call resets its timer: notifications naming another token
/// do not keep it alive.
#[tokio::test]
async fn progress_for_another_token_does_not_reset_the_idle_timeout() {
    let installation_id = Uuid::new_v4();
    let server = start_server(Reply::OtherTokenProgress).await;
    let service = connect_with_progress_routing(&server, installation_id).await;
    let timeout = Duration::from_millis(400);
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        call_tool_with_deadline_inner(
            Uuid::nil(),
            installation_id,
            CallToolRequestParams::new("test"),
            connected(service.peer().clone()),
            limits(timeout),
            move || tokio::time::sleep(timeout),
            pending,
            || ready(()),
        ),
    )
    .await
    .expect("progress for another call must not keep this one alive");
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("outcome is unknown")
    );
    service.cancellation_token().cancel();
}

/// The public entry point runs on the timeout it is given (the server's override), not
/// the default.
#[tokio::test]
async fn call_tool_with_deadline_uses_the_given_timeout() {
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        call_tool_with_deadline(
            Uuid::nil(),
            Uuid::nil(),
            CallToolRequestParams::new("test"),
            ToolCallLimits::for_idle_timeout(Duration::from_secs(1)),
            pending,
        ),
    )
    .await
    .expect("a 1 s limit must not wait for the 5 minute default");
    let message = result.unwrap_err().to_string();
    assert!(message.contains("after 1 seconds"), "{message}");
    assert!(message.contains("not dispatched"), "{message}");
}

#[test]
fn default_tool_call_timeout_is_five_minutes() {
    assert_eq!(DEFAULT_TOOL_CALL_TIMEOUT, Duration::from_secs(300));
    assert_eq!(tool_call_timeout(None), DEFAULT_TOOL_CALL_TIMEOUT);
}

#[test]
fn tool_timeout_secs_override_is_honoured_and_clamped() {
    assert_eq!(
        tool_call_timeout(NonZeroU64::new(42)),
        Duration::from_secs(42)
    );
    assert_eq!(
        tool_call_timeout(NonZeroU64::new(MAX_TOOL_CALL_TIMEOUT.as_secs())),
        MAX_TOOL_CALL_TIMEOUT
    );
    assert_eq!(
        tool_call_timeout(NonZeroU64::new(u64::MAX)),
        MAX_TOOL_CALL_TIMEOUT
    );
}

#[test]
fn tool_timeout_secs_is_read_from_every_accepted_config_shape() {
    use crate::ai::mcp::MCPServer;
    for json in [
        r#"{"srv": {"command": "x", "tool_timeout_secs": 42}}"#,
        r#"{"mcpServers": {"srv": {"command": "x", "tool_timeout_secs": 42}}}"#,
        r#"{"servers": {"srv": {"url": "http://localhost/mcp", "tool_timeout_secs": 42}}}"#,
        r#""srv": {"command": "x", "tool_timeout_secs": 42}"#,
    ] {
        assert_eq!(
            MCPServer::tool_timeout_secs_from_user_json(json),
            Ok(NonZeroU64::new(42)),
            "{json}"
        );
    }
}

#[test]
fn missing_tool_timeout_secs_means_the_default() {
    use crate::ai::mcp::MCPServer;
    for json in [
        r#"{"srv": {"command": "x"}}"#,
        r#"{"srv": {"command": "x", "tool_timeout_secs": null}}"#,
    ] {
        let secs = MCPServer::tool_timeout_secs_from_user_json(json).unwrap();
        assert_eq!(secs, None, "{json}");
        assert_eq!(tool_call_timeout(secs), DEFAULT_TOOL_CALL_TIMEOUT);
    }
}

#[test]
fn invalid_tool_timeout_secs_is_rejected() {
    use crate::ai::mcp::MCPServer;
    for value in ["0", "-5", "1.5", "\"60\"", "true"] {
        let json = format!(r#"{{"srv": {{"command": "x", "tool_timeout_secs": {value}}}}}"#);
        let err = MCPServer::tool_timeout_secs_from_user_json(&json).unwrap_err();
        assert!(err.contains("tool_timeout_secs"), "{err}");
        assert!(err.contains("srv"), "{err}");
    }
}

/// The override survives the trip into a stored templatable server, whose JSON is what
/// the server is started from, and does not stop the rest of the config parsing.
#[test]
fn tool_timeout_secs_round_trips_through_a_templatable_server() {
    use crate::ai::mcp::{MCPServer, ParsedTemplatableMCPServerResult};
    let parsed = ParsedTemplatableMCPServerResult::from_user_json(
        r#"{"mcpServers": {"srv": {"command": "x", "env": {"K": "v"}, "tool_timeout_secs": 900}}}"#,
    )
    .unwrap();
    assert_eq!(parsed.len(), 1);
    let template_json = &parsed[0].templatable_mcp_server.template.json;
    assert_eq!(
        MCPServer::tool_timeout_secs_from_user_json(template_json),
        Ok(NonZeroU64::new(900)),
        "{template_json}"
    );
    let servers = MCPServer::from_user_json(template_json).unwrap();
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0].name, "srv");
}

#[test]
fn total_limit_is_ten_idle_periods_and_at_least_an_hour() {
    let hour = Duration::from_secs(60 * 60);
    assert_eq!(MIN_TOTAL_TOOL_CALL_TIMEOUT, hour);
    assert_eq!(total_tool_call_timeout(DEFAULT_TOOL_CALL_TIMEOUT), hour);
    assert_eq!(total_tool_call_timeout(Duration::from_secs(1)), hour);
    assert_eq!(total_tool_call_timeout(2 * hour), 20 * hour);
    assert_eq!(
        total_tool_call_timeout(MAX_TOOL_CALL_TIMEOUT),
        MAX_TOOL_CALL_TIMEOUT * 10
    );
    assert_eq!(
        limits(DEFAULT_TOOL_CALL_TIMEOUT),
        ToolCallLimits {
            idle: DEFAULT_TOOL_CALL_TIMEOUT,
            total: hour,
        }
    );
}

/// Progress keeps resetting the idle timer, but not the total limit: a server that
/// reports progress forever and never answers still ends the call.
#[tokio::test]
async fn progress_forever_hits_the_total_limit() {
    let installation_id = Uuid::new_v4();
    let server = start_server(Reply::ProgressForever).await;
    let service = connect_with_progress_routing(&server, installation_id).await;
    let idle = Duration::from_millis(300);
    let total = Duration::from_millis(1200);
    let started = std::time::Instant::now();
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        call_tool_with_deadline_inner(
            Uuid::nil(),
            installation_id,
            CallToolRequestParams::new("test"),
            connected(service.peer().clone()),
            ToolCallLimits { idle, total },
            move || tokio::time::sleep(idle),
            move || tokio::time::sleep(total),
            pending,
        ),
    )
    .await
    .expect("the total limit must end a call that only reports progress");
    let message = result.unwrap_err().to_string();
    assert!(message.contains("reporting progress"), "{message}");
    assert!(message.contains("1 seconds"), "{message}");
    assert!(message.contains("asked to cancel"), "{message}");
    assert!(message.contains("outcome is unknown"), "{message}");
    // The idle limit alone would have ended the call after 300 ms.
    assert!(started.elapsed() >= total, "{:?}", started.elapsed());
    assert_eq!(server.state.calls.load(Ordering::SeqCst), 1);
    assert_eq!(server.state.cancellations.load(Ordering::SeqCst), 1);
    service.cancellation_token().cancel();
}

/// A slow connect does not eat into the dispatched call's idle period: the idle timer is
/// re-armed when the request is queued.
#[tokio::test]
async fn slow_connect_does_not_shorten_the_idle_period() {
    let server = start_server(Reply::WrongId).await;
    let service =
        ().serve(StreamableHttpClientTransport::from_uri(server.uri.clone()))
            .await
            .unwrap();
    let peer = service.peer().clone();
    let idle = Duration::from_millis(400);
    let connect_delay = Duration::from_millis(300);
    let started = std::time::Instant::now();
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        call_tool_with_deadline_inner(
            Uuid::nil(),
            Uuid::nil(),
            CallToolRequestParams::new("test"),
            move || {
                let peer = peer.clone();
                async move {
                    tokio::time::sleep(connect_delay).await;
                    Ok(peer)
                }
            },
            limits(idle),
            move || tokio::time::sleep(idle),
            pending,
            || ready(()),
        ),
    )
    .await
    .unwrap();
    let message = result.unwrap_err().to_string();
    assert!(message.contains("outcome is unknown"), "{message}");
    assert!(
        started.elapsed() >= connect_delay + idle,
        "{:?}",
        started.elapsed()
    );
    assert_eq!(server.state.calls.load(Ordering::SeqCst), 1);
    service.cancel().await.unwrap();
}

/// A peer whose connection has already shut down: sending on it fails before the request
/// is queued.
async fn closed_peer(server: &TestServer) -> Peer<RoleClient> {
    let service =
        ().serve(StreamableHttpClientTransport::from_uri(server.uri.clone()))
            .await
            .unwrap();
    let peer = service.peer().clone();
    service.cancel().await.unwrap();
    assert!(peer.is_transport_closed());
    peer
}

/// A `TransportClosed` from the send step means the request never entered the
/// connection, so the call reconnects and sends it once more.
#[tokio::test]
async fn send_step_transport_closed_reconnects_and_sends_once_more() {
    let server = start_server(Reply::Success).await;
    let dead = closed_peer(&server).await;
    let live = ().serve(StreamableHttpClientTransport::from_uri(server.uri.clone())).await.unwrap();
    let connects = Arc::new(AtomicUsize::new(0));
    let mut peers = vec![live.peer().clone(), dead];
    let connect = {
        let connects = connects.clone();
        move || {
            connects.fetch_add(1, Ordering::SeqCst);
            ready(Ok(peers.pop().expect("at most two connects")))
        }
    };
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        call_tool_with_deadline_inner(
            Uuid::nil(),
            Uuid::nil(),
            CallToolRequestParams::new("test"),
            connect,
            limits(DEFAULT_TOOL_CALL_TIMEOUT),
            pending,
            pending,
            || ready(()),
        ),
    )
    .await
    .unwrap();
    assert_eq!(result.unwrap().is_error, Some(false));
    assert_eq!(connects.load(Ordering::SeqCst), 2);
    assert_eq!(server.state.calls.load(Ordering::SeqCst), 1);
    live.cancel().await.unwrap();
}

/// The send-step retry happens once: a second `TransportClosed` fails the call.
#[tokio::test]
async fn send_step_transport_closed_is_retried_only_once() {
    let server = start_server(Reply::Success).await;
    let dead = closed_peer(&server).await;
    let connects = Arc::new(AtomicUsize::new(0));
    let connect = {
        let connects = connects.clone();
        move || {
            connects.fetch_add(1, Ordering::SeqCst);
            ready(Ok(dead.clone()))
        }
    };
    let result = call_tool_with_deadline_inner(
        Uuid::nil(),
        Uuid::nil(),
        CallToolRequestParams::new("test"),
        connect,
        limits(DEFAULT_TOOL_CALL_TIMEOUT),
        pending,
        pending,
        || ready(()),
    )
    .await;
    assert!(matches!(result, Err(ServiceError::TransportClosed)));
    assert_eq!(connects.load(Ordering::SeqCst), 2);
    assert_eq!(server.state.calls.load(Ordering::SeqCst), 0);
}

/// A connection that breaks off after `tools/call` was sent is never replayed: the
/// server sees one call, there is one connect, and the model is told the outcome is
/// unknown, whether rmcp reports the lost connection or the idle limit ends the wait.
#[tokio::test]
async fn dropped_connection_after_dispatch_is_not_replayed() {
    let server = start_server(Reply::DropOnCall).await;
    let service =
        ().serve(StreamableHttpClientTransport::from_uri(server.uri.clone()))
            .await
            .unwrap();
    let peer = service.peer().clone();
    let connects = Arc::new(AtomicUsize::new(0));
    let connect = {
        let connects = connects.clone();
        move || {
            connects.fetch_add(1, Ordering::SeqCst);
            ready(Ok(peer.clone()))
        }
    };
    let idle = Duration::from_millis(500);
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        call_tool_with_deadline_inner(
            Uuid::nil(),
            Uuid::nil(),
            CallToolRequestParams::new("test"),
            connect,
            limits(idle),
            move || tokio::time::sleep(idle),
            pending,
            || ready(()),
        ),
    )
    .await
    .expect("a dropped connection must not leave the tool call pending");
    let message = result.unwrap_err().to_string();
    assert!(message.contains("outcome is unknown"), "{message}");
    assert!(message.contains("Do not retry"), "{message}");
    assert_eq!(connects.load(Ordering::SeqCst), 1);
    assert_eq!(server.state.calls.load(Ordering::SeqCst), 1);
    service.cancellation_token().cancel();
}
