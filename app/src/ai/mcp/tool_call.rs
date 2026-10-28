//! A bounded MCP tool call (winters-phosphor/phosphor#1064).
//!
//! Without a deadline, a server that accepts a `tools/call` and never answers left the
//! agent turn waiting forever. rmcp 2.2 also leaves a call pending when an SSE response
//! ends without a result, so a merely flaky HTTP server could do the same. Ported from
//! upstream warpdotdev/warp `d5d23e611` (PR #16219), minus its rmcp 3.5 upgrade, and
//! changed in two ways:
//!
//! - The limit is an *idle* timeout, not a total one. Every `notifications/progress` the
//!   server sends for the call restarts it, so a long tool that reports progress keeps
//!   running while a silent one times out. rmcp attaches a `progressToken` to every
//!   request's `_meta`; [`McpClientHandler`] routes the server's progress notifications
//!   back to the call that owns the token.
//! - The limit is [`DEFAULT_TOOL_CALL_TIMEOUT`] (5 minutes) unless the server's MCP config
//!   sets `tool_timeout_secs` (see [`tool_call_timeout`]).
//!
//! rmcp 2.2 can reset a request timeout on progress itself
//! (`PeerRequestOptions::reset_timeout_on_progress`), but on expiry it awaits delivery of
//! its cancellation notification without a bound, so a stalled send would still hang the
//! call. The timer and the progress routing therefore live here.

use std::collections::HashMap;
use std::future::Future;
use std::num::NonZeroU64;
use std::sync::LazyLock;
use std::time::Duration;

use futures::channel::mpsc;
use futures::future::{Either, pending, select};
use futures::{FutureExt, StreamExt};
use instant::Instant;
use parking_lot::Mutex;
use rmcp::model::{
    CallToolRequest, CallToolRequestParams, CallToolResult, CancelledNotificationParam, ClientInfo,
    ClientRequest, ErrorData, ProgressNotificationParam, ProgressToken, RequestId, ServerResult,
};
use rmcp::service::{MaybeSendFuture, NotificationContext, PeerRequestOptions};
use rmcp::{ClientHandler, Peer, RoleClient, ServiceError};
use uuid::Uuid;
use warpui::r#async::Timer;

/// How long a tool call may go without a result or a progress notification, unless the
/// server's config sets `tool_timeout_secs`.
pub const DEFAULT_TOOL_CALL_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// The largest `tool_timeout_secs` honoured; larger values are clamped to it. A day is
/// effectively "no limit" for an interactive tool call, and keeps the timer arithmetic
/// far from `Instant` overflow.
pub const MAX_TOOL_CALL_TIMEOUT: Duration = Duration::from_secs(24 * 60 * 60);
/// The MCP server config key that overrides [`DEFAULT_TOOL_CALL_TIMEOUT`].
pub const TOOL_TIMEOUT_SECS_KEY: &str = "tool_timeout_secs";
/// How long a timed-out call waits for its `notifications/cancelled` to be delivered.
const CANCELLATION_TIMEOUT: Duration = Duration::from_secs(5);

/// The idle timeout for a server's tool calls: its `tool_timeout_secs` override, clamped
/// to [`MAX_TOOL_CALL_TIMEOUT`], or [`DEFAULT_TOOL_CALL_TIMEOUT`] when it sets none.
pub fn tool_call_timeout(tool_timeout_secs: Option<NonZeroU64>) -> Duration {
    let Some(secs) = tool_timeout_secs else {
        return DEFAULT_TOOL_CALL_TIMEOUT;
    };
    let requested = Duration::from_secs(secs.get());
    if requested > MAX_TOOL_CALL_TIMEOUT {
        log::warn!(
            "MCP {TOOL_TIMEOUT_SECS_KEY}={} exceeds the maximum; using {} seconds",
            secs.get(),
            MAX_TOOL_CALL_TIMEOUT.as_secs()
        );
        return MAX_TOOL_CALL_TIMEOUT;
    }
    requested
}

/// Calls a tool once. The call fails once it has gone `timeout` without a result or a
/// progress notification from the server, connecting included.
///
/// The call is never replayed: after dispatch the tool may have run even when its
/// response is lost, so a retry could execute a side-effecting tool twice.
pub async fn call_tool_with_deadline(
    operation_id: Uuid,
    installation_id: Uuid,
    params: CallToolRequestParams,
    timeout: Duration,
    connect: impl Future<Output = Result<Peer<RoleClient>, ServiceError>>,
) -> Result<CallToolResult, ServiceError> {
    call_tool_with_deadline_inner(
        operation_id,
        installation_id,
        params,
        connect,
        timeout,
        move || futures::FutureExt::map(Timer::after(timeout), |_| ()),
        || futures::FutureExt::map(Timer::after(CANCELLATION_TIMEOUT), |_| ()),
    )
    .await
}

/// What ended one wait for the response.
enum WaitEvent {
    Response(Result<ServerResult, ServiceError>),
    Deadline,
    Progress,
    ProgressClosed,
}

/// `idle_deadline` starts one idle period; it is called once up front and again after
/// every progress notification for the call.
async fn call_tool_with_deadline_inner<
    Connect,
    Idle,
    IdleTimer,
    CancellationDeadline,
    CancellationTimer,
>(
    operation_id: Uuid,
    installation_id: Uuid,
    params: CallToolRequestParams,
    connect: Connect,
    timeout: Duration,
    mut idle_deadline: Idle,
    cancellation_deadline: CancellationDeadline,
) -> Result<CallToolResult, ServiceError>
where
    Connect: Future<Output = Result<Peer<RoleClient>, ServiceError>>,
    Idle: FnMut() -> IdleTimer,
    IdleTimer: Future<Output = ()>,
    CancellationDeadline: FnOnce() -> CancellationTimer,
    CancellationTimer: Future<Output = ()>,
{
    let mut lifecycle = ToolCallLifecycle {
        operation_id,
        installation_id,
        started_at: Instant::now(),
        timeout,
        request_id: None,
        progress_notifications: 0,
        phase: "connecting",
        outcome: "pending",
    };
    lifecycle.log("started");
    let mut timer = Box::pin(idle_deadline());

    // Connecting and dispatching share the first idle period.
    let dispatched = {
        let dispatch = async {
            let peer = connect.await?;
            lifecycle.phase = "dispatching";
            lifecycle.log("peer_ready");
            let request = ClientRequest::CallToolRequest(CallToolRequest::new(params));
            // rmcp's own request timeout awaits delivery of its cancellation before
            // returning. Keep the deadline outside that path so a stalled send cannot
            // defeat it.
            let handle = peer
                .send_cancellable_request(request, PeerRequestOptions::no_options())
                .await?;
            Ok::<_, ServiceError>((peer, handle))
        };
        match select(Box::pin(dispatch), &mut timer).await {
            Either::Left((result, _)) => Some(result),
            Either::Right(((), _)) => None,
        }
    };

    let mut cancellation: Option<(Peer<RoleClient>, RequestId)> = None;
    let completed = match dispatched {
        None => None,
        Some(Err(err)) => Some(Err(err)),
        Some(Ok((peer, handle))) => {
            let request_id = handle.id.clone();
            lifecycle.request_id = Some(request_id.clone());
            lifecycle.phase = "awaiting_response";
            lifecycle.log("request_enqueued");
            // A progress notification that beats this subscription is not counted; the
            // next one resets the timer.
            let mut progress = ProgressSubscription::new(installation_id, &handle.progress_token);
            let mut progress_open = true;
            let mut response = Box::pin(handle.await_response());
            let response = loop {
                let event = {
                    let next_progress = std::pin::pin!(async {
                        if progress_open {
                            progress.rx.next().await
                        } else {
                            pending().await
                        }
                    });
                    match select(&mut response, select(&mut timer, next_progress)).await {
                        Either::Left((result, _)) => WaitEvent::Response(result),
                        Either::Right((Either::Left(((), _)), _)) => WaitEvent::Deadline,
                        Either::Right((Either::Right((Some(()), _)), _)) => WaitEvent::Progress,
                        Either::Right((Either::Right((None, _)), _)) => WaitEvent::ProgressClosed,
                    }
                };
                match event {
                    WaitEvent::Response(result) => break Some(result),
                    WaitEvent::Deadline => break None,
                    WaitEvent::Progress => {
                        lifecycle.progress_notifications += 1;
                        log::debug!(
                            "MCP tool lifecycle: event=progress operation_id={operation_id} count={}",
                            lifecycle.progress_notifications
                        );
                        timer = Box::pin(idle_deadline());
                    }
                    // Another call took over this progress token (only possible across a
                    // reconnect); keep waiting on the current idle period.
                    WaitEvent::ProgressClosed => progress_open = false,
                }
            };
            match response {
                Some(result) => Some(result.and_then(|result| match result {
                    ServerResult::CallToolResult(result) => Ok(result),
                    _ => Err(ServiceError::UnexpectedResponse),
                })),
                None => {
                    cancellation = Some((peer, request_id));
                    None
                }
            }
        }
    };

    if let Some(result) = completed {
        lifecycle.outcome = match &result {
            Ok(result) if result.is_error == Some(true) => "tool_error",
            Ok(_) => "success",
            Err(_) => "service_error",
        };
        return result;
    }

    lifecycle.outcome = "deadline_exceeded";
    lifecycle.log("deadline");
    if let Some((peer, request_id)) = cancellation {
        let cancel = peer.notify_cancelled(CancelledNotificationParam::new(
            Some(request_id),
            Some("Phosphor MCP tool call timed out".to_owned()),
        ));
        let cancellation_outcome =
            match select(Box::pin(cancel), Box::pin(cancellation_deadline())).await {
                Either::Left((Ok(()), _)) => "cancellation_sent",
                Either::Left((Err(_), _)) => "cancellation_failed",
                Either::Right(((), _)) => "cancellation_deadline_exceeded",
            };
        lifecycle.log(cancellation_outcome);
    }

    Err(ServiceError::McpError(ErrorData::internal_error(
        deadline_message(lifecycle.phase == "connecting", timeout),
        None,
    )))
}

/// The tool error the model (and the tool call's UI) sees when a call runs out of time.
fn deadline_message(still_connecting: bool, timeout: Duration) -> String {
    let secs = timeout.as_secs();
    if still_connecting {
        format!(
            "MCP tool call timed out after {secs} seconds while connecting. The tool was not dispatched."
        )
    } else {
        format!(
            "MCP tool call timed out after {secs} seconds without a result or progress from the server. The tool may have executed; its outcome is unknown. Do not retry automatically. (The limit can be raised with `{TOOL_TIMEOUT_SECS_KEY}` in this server's MCP config.)"
        )
    }
}

/// The client side of every MCP connection: the client info sent in `initialize`, plus
/// routing of `notifications/progress` to the tool call that owns the progress token.
pub struct McpClientHandler {
    info: ClientInfo,
    installation_id: Uuid,
}

impl McpClientHandler {
    pub fn new(info: ClientInfo, installation_id: Uuid) -> Self {
        Self {
            info,
            installation_id,
        }
    }
}

impl ClientHandler for McpClientHandler {
    fn get_info(&self) -> ClientInfo {
        self.info.clone()
    }

    fn on_progress(
        &self,
        params: ProgressNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) -> impl Future<Output = ()> + MaybeSendFuture + '_ {
        route_progress(self.installation_id, &params.progress_token);
        std::future::ready(())
    }
}

/// Progress tokens are only unique per connection, so routes are keyed by installation
/// as well. Each route carries the id of the subscription that owns it, so a superseded
/// subscription's drop cannot remove its successor's route.
type ProgressKey = (Uuid, ProgressToken);

#[derive(Default)]
struct ProgressRoutes {
    next_id: u64,
    routes: HashMap<ProgressKey, (u64, mpsc::UnboundedSender<()>)>,
}

static PROGRESS_ROUTES: LazyLock<Mutex<ProgressRoutes>> = LazyLock::new(Default::default);

/// Forwards one progress notification to the in-flight call that owns `token`, if any.
fn route_progress(installation_id: Uuid, token: &ProgressToken) {
    let routes = PROGRESS_ROUTES.lock();
    if let Some((_, tx)) = routes.routes.get(&(installation_id, token.clone())) {
        let _ = tx.unbounded_send(());
    }
}

/// An in-flight call's claim on its progress token; dropping it removes the route.
struct ProgressSubscription {
    key: ProgressKey,
    id: u64,
    rx: mpsc::UnboundedReceiver<()>,
}

impl ProgressSubscription {
    fn new(installation_id: Uuid, token: &ProgressToken) -> Self {
        let key = (installation_id, token.clone());
        let (tx, rx) = mpsc::unbounded();
        let mut routes = PROGRESS_ROUTES.lock();
        let id = routes.next_id;
        routes.next_id += 1;
        routes.routes.insert(key.clone(), (id, tx));
        Self { key, id, rx }
    }
}

impl Drop for ProgressSubscription {
    fn drop(&mut self) {
        let mut routes = PROGRESS_ROUTES.lock();
        if routes
            .routes
            .get(&self.key)
            .is_some_and(|(id, _)| *id == self.id)
        {
            routes.routes.remove(&self.key);
        }
    }
}

struct ToolCallLifecycle {
    operation_id: Uuid,
    installation_id: Uuid,
    started_at: Instant,
    timeout: Duration,
    request_id: Option<RequestId>,
    progress_notifications: u64,
    phase: &'static str,
    outcome: &'static str,
}

impl ToolCallLifecycle {
    fn log(&self, event: &str) {
        log::info!(
            "MCP tool lifecycle: event={event} operation_id={} installation_id={} request_id={:?} phase={} idle_timeout_s={} progress_notifications={} elapsed_ms={} outcome={}",
            self.operation_id,
            self.installation_id,
            self.request_id,
            self.phase,
            self.timeout.as_secs(),
            self.progress_notifications,
            self.started_at.elapsed().as_millis(),
            self.outcome,
        );
    }
}

impl Drop for ToolCallLifecycle {
    fn drop(&mut self) {
        // A call dropped while pending was abandoned, e.g. by the user pressing Stop.
        if self.outcome == "pending" {
            self.outcome = "dropped";
        }
        self.log("finished");
    }
}

#[cfg(test)]
#[path = "tool_call_tests.rs"]
mod tests;
