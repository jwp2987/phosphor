//! A bounded MCP tool call (winters-phosphor/phosphor#1064).
//!
//! Without a deadline, a server that accepts a `tools/call` and never answers left the
//! agent turn waiting forever. rmcp 2.2 also leaves a call pending when an SSE response
//! ends without a result, so a merely flaky HTTP server could do the same. Ported from
//! upstream warpdotdev/warp `d5d23e611` (PR #16219), minus its rmcp 3.5 upgrade, and
//! changed in three ways:
//!
//! - The main limit is an *idle* timeout. Every `notifications/progress` the server sends
//!   for the call restarts it, so a long tool that reports progress keeps running while a
//!   silent one times out. A total limit that progress does not reset
//!   ([`total_tool_call_timeout`]) still ends a call that reports progress forever. rmcp
//!   attaches a `progressToken` to every request's `_meta`; [`McpClientHandler`] routes
//!   the server's progress notifications back to the call that owns the token.
//! - The idle limit is [`DEFAULT_TOOL_CALL_TIMEOUT`] (5 minutes) unless the server's MCP
//!   config sets `tool_timeout_secs` (see [`tool_call_timeout`]).
//! - A request that could not even be queued on the connection is sent once more after
//!   reconnecting; once queued it is never replayed.
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

use futures::StreamExt;
use futures::channel::mpsc;
use futures::future::{Either, pending, select};
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
/// The least total limit on a dispatched call (see [`total_tool_call_timeout`]).
pub const MIN_TOTAL_TOOL_CALL_TIMEOUT: Duration = Duration::from_secs(60 * 60);
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

/// The two limits on one tool call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolCallLimits {
    /// How long the call may go without a result or a progress notification. Connecting
    /// and dispatching get one such period of their own.
    pub idle: Duration,
    /// How long a dispatched call may run in all, however much progress it reports.
    pub total: Duration,
}

impl ToolCallLimits {
    /// The limits for a server whose idle timeout is `idle`.
    pub fn for_idle_timeout(idle: Duration) -> Self {
        Self {
            idle,
            total: total_tool_call_timeout(idle),
        }
    }
}

/// The total limit for a dispatched call: ten idle periods, and never less than
/// [`MIN_TOTAL_TOOL_CALL_TIMEOUT`]. Progress does not reset it, so a server that reports
/// progress forever without answering still cannot block the turn forever.
pub fn total_tool_call_timeout(idle: Duration) -> Duration {
    idle.saturating_mul(10).max(MIN_TOTAL_TOOL_CALL_TIMEOUT)
}

/// Calls a tool once, bounded by `limits`: connecting and dispatching may take one idle
/// period, and once dispatched the call fails after an idle period without a result or a
/// progress notification, or after the total limit, whichever comes first.
///
/// `connect` is called again only when the request could not be queued on the
/// connection at all (a `TransportClosed` from the send step), so the server cannot have
/// seen it. Once the request is queued the call is never replayed: the tool may have run
/// even when its response is lost, so a retry could execute a side-effecting tool twice.
pub async fn call_tool_with_deadline<Connect, ConnectFuture>(
    operation_id: Uuid,
    installation_id: Uuid,
    params: CallToolRequestParams,
    limits: ToolCallLimits,
    connect: Connect,
) -> Result<CallToolResult, ServiceError>
where
    Connect: FnMut() -> ConnectFuture,
    ConnectFuture: Future<Output = Result<Peer<RoleClient>, ServiceError>>,
{
    // `async_io::Timer` is both a `Future` and a `Stream`, so `FutureExt::map` on it is
    // ambiguous; await it in a block instead.
    let ToolCallLimits { idle, total } = limits;
    call_tool_with_deadline_inner(
        operation_id,
        installation_id,
        params,
        connect,
        limits,
        move || async move {
            Timer::after(idle).await;
        },
        move || async move {
            Timer::after(total).await;
        },
        || async {
            Timer::after(CANCELLATION_TIMEOUT).await;
        },
    )
    .await
}

/// What ended one wait for the response.
enum WaitEvent {
    Response(Result<ServerResult, ServiceError>),
    IdleDeadline,
    TotalDeadline,
    Progress,
    ProgressClosed,
}

/// Which limit a call ran out of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Expiry {
    /// Connecting or queueing the request took an idle period; nothing was sent.
    Connecting,
    /// The dispatched call went an idle period without a result or progress.
    Idle,
    /// The dispatched call reached the total limit while reporting progress.
    Total,
}

/// `idle_deadline` starts one idle period: once for connecting and dispatching, again
/// when the request is queued, and again after every progress notification for the call.
/// `total_deadline` is started once, when the request is queued.
#[allow(clippy::too_many_arguments)]
async fn call_tool_with_deadline_inner<
    Connect,
    ConnectFuture,
    Idle,
    IdleTimer,
    Total,
    TotalTimer,
    CancellationDeadline,
    CancellationTimer,
>(
    operation_id: Uuid,
    installation_id: Uuid,
    params: CallToolRequestParams,
    mut connect: Connect,
    limits: ToolCallLimits,
    mut idle_deadline: Idle,
    total_deadline: Total,
    cancellation_deadline: CancellationDeadline,
) -> Result<CallToolResult, ServiceError>
where
    Connect: FnMut() -> ConnectFuture,
    ConnectFuture: Future<Output = Result<Peer<RoleClient>, ServiceError>>,
    Idle: FnMut() -> IdleTimer,
    IdleTimer: Future<Output = ()>,
    Total: FnOnce() -> TotalTimer,
    TotalTimer: Future<Output = ()>,
    CancellationDeadline: FnOnce() -> CancellationTimer,
    CancellationTimer: Future<Output = ()>,
{
    let mut lifecycle = ToolCallLifecycle {
        operation_id,
        installation_id,
        started_at: Instant::now(),
        limits,
        request_id: None,
        progress_notifications: 0,
        phase: "connecting",
        outcome: "pending",
    };
    lifecycle.log("started");
    let mut timer = Box::pin(idle_deadline());

    // Connecting and queueing the request get one idle period of their own.
    let dispatched = {
        let dispatch = async {
            let mut resent = false;
            loop {
                let peer = connect().await?;
                lifecycle.phase = "dispatching";
                lifecycle.log("peer_ready");
                let request = ClientRequest::CallToolRequest(CallToolRequest::new(params.clone()));
                // rmcp's own request timeout awaits delivery of its cancellation before
                // returning. Keep the deadline outside that path so a stalled send cannot
                // defeat it.
                match peer
                    .send_cancellable_request(request, PeerRequestOptions::no_options())
                    .await
                {
                    Ok(handle) => break Ok::<_, ServiceError>((peer, handle)),
                    // The request never entered the connection's queue (rmcp fails the
                    // send itself), so the server cannot have seen it: reconnect and send
                    // it once more.
                    Err(ServiceError::TransportClosed) if !resent => {
                        resent = true;
                        lifecycle.phase = "connecting";
                        lifecycle.log("send_transport_closed_reconnecting");
                    }
                    Err(err) => break Err(err),
                }
            }
        };
        match select(Box::pin(dispatch), &mut timer).await {
            Either::Left((result, _)) => Some(result),
            Either::Right(((), _)) => None,
        }
    };

    let mut cancellation: Option<(Peer<RoleClient>, RequestId)> = None;
    let completed: Result<Result<CallToolResult, ServiceError>, Expiry> = match dispatched {
        None => Err(Expiry::Connecting),
        Some(Err(err)) => Ok(Err(err)),
        Some(Ok((peer, handle))) => {
            let request_id = handle.id.clone();
            lifecycle.request_id = Some(request_id.clone());
            lifecycle.phase = "awaiting_response";
            lifecycle.log("request_enqueued");
            // The call is now with the server: a fresh idle period, and the total limit
            // starts, so a slow connect does not eat into either.
            timer = Box::pin(idle_deadline());
            let mut total = Box::pin(total_deadline());
            // A progress notification that beats this subscription is not counted; the
            // next one resets the timer.
            let mut progress = ProgressSubscription::new(installation_id, &handle.progress_token);
            let mut progress_open = true;
            let mut response = Box::pin(handle.await_response());
            let ended = loop {
                let event = {
                    let next_progress = std::pin::pin!(async {
                        if progress_open {
                            progress.rx.next().await
                        } else {
                            pending().await
                        }
                    });
                    let deadlines = select(&mut timer, select(&mut total, next_progress));
                    match select(&mut response, deadlines).await {
                        Either::Left((result, _)) => WaitEvent::Response(result),
                        Either::Right((Either::Left(((), _)), _)) => WaitEvent::IdleDeadline,
                        Either::Right((Either::Right((Either::Left(((), _)), _)), _)) => {
                            WaitEvent::TotalDeadline
                        }
                        Either::Right((Either::Right((Either::Right((Some(()), _)), _)), _)) => {
                            WaitEvent::Progress
                        }
                        Either::Right((Either::Right((Either::Right((None, _)), _)), _)) => {
                            WaitEvent::ProgressClosed
                        }
                    }
                };
                match event {
                    WaitEvent::Response(result) => break Ok(result),
                    WaitEvent::IdleDeadline => break Err(Expiry::Idle),
                    WaitEvent::TotalDeadline => break Err(Expiry::Total),
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
            match ended {
                Ok(Ok(ServerResult::CallToolResult(result))) => Ok(Ok(result)),
                Ok(Ok(_)) => Ok(Err(ServiceError::UnexpectedResponse)),
                // The request was queued, so the server may have run the tool before the
                // connection went away.
                Ok(Err(err @ (ServiceError::TransportClosed | ServiceError::TransportSend(_)))) => {
                    Ok(Err(outcome_unknown_error(&err)))
                }
                Ok(Err(err)) => Ok(Err(err)),
                Err(expiry) => {
                    cancellation = Some((peer, request_id));
                    Err(expiry)
                }
            }
        }
    };

    let expiry = match completed {
        Ok(result) => {
            lifecycle.outcome = match &result {
                Ok(result) if result.is_error == Some(true) => "tool_error",
                Ok(_) => "success",
                Err(_) => "service_error",
            };
            return result;
        }
        Err(expiry) => expiry,
    };

    lifecycle.outcome = match expiry {
        Expiry::Connecting => "connect_timeout",
        Expiry::Idle => "idle_timeout",
        Expiry::Total => "total_timeout",
    };
    lifecycle.log("deadline");
    let cancel_requested = cancellation.is_some();
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
        deadline_message(expiry, limits, cancel_requested),
        None,
    )))
}

const OUTCOME_UNKNOWN: &str =
    "The tool may have executed; its outcome is unknown. Do not retry automatically.";

/// The tool error the model (and the tool call's UI) sees when a call runs out of time.
fn deadline_message(expiry: Expiry, limits: ToolCallLimits, cancel_requested: bool) -> String {
    let cancel = if cancel_requested {
        " The server was asked to cancel the call."
    } else {
        ""
    };
    match expiry {
        Expiry::Connecting => format!(
            "MCP tool call timed out after {} seconds while connecting to the server. The tool was not dispatched.",
            limits.idle.as_secs()
        ),
        Expiry::Idle => format!(
            "MCP tool call timed out after {} seconds without a result or progress from the server.{cancel} {OUTCOME_UNKNOWN} (The limit can be raised with `{TOOL_TIMEOUT_SECS_KEY}` in this server's MCP config.)",
            limits.idle.as_secs()
        ),
        Expiry::Total => format!(
            "MCP tool call ran for {} seconds, reporting progress, without returning a result.{cancel} {OUTCOME_UNKNOWN}",
            limits.total.as_secs()
        ),
    }
}

/// The error for a call whose connection was lost after the request was queued.
fn outcome_unknown_error(err: &ServiceError) -> ServiceError {
    ServiceError::McpError(ErrorData::internal_error(
        format!(
            "The connection to the MCP server was lost after the tool call was sent ({err}). {OUTCOME_UNKNOWN}"
        ),
        None,
    ))
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
    limits: ToolCallLimits,
    request_id: Option<RequestId>,
    progress_notifications: u64,
    phase: &'static str,
    outcome: &'static str,
}

impl ToolCallLifecycle {
    fn log(&self, event: &str) {
        log::info!(
            "MCP tool lifecycle: event={event} operation_id={} installation_id={} request_id={:?} phase={} idle_timeout_s={} total_timeout_s={} progress_notifications={} elapsed_ms={} outcome={}",
            self.operation_id,
            self.installation_id,
            self.request_id,
            self.phase,
            self.limits.idle.as_secs(),
            self.limits.total.as_secs(),
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
