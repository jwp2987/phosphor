//! Per-host daemon-owned pty session lists, for the remote-hosts dashboard
//! (`docs/design/moth-parliament.md`, "the daemon holds the pty").
//!
//! `RemoteServerClient::list_sessions` had no caller in `app` at all: a client
//! could enumerate a host's sessions but nothing ever did, so a session that
//! outlived the tab that spawned it was invisible. This model is that caller,
//! and the hosts dashboard (`pane_group/pane/remote_hosts_pane.rs`) is its only
//! consumer.
//!
//! # It never dials
//!
//! This is the rule the dashboard is built around, and the one thing here that
//! is not negotiable. [`HostRegistryModel::refresh_from_live_connections`]
//! (`remote_server/host_registry.rs`) explains why: dialling a fresh SSH
//! connection to a host just because someone opened a dashboard "would be worse
//! than no refresh at all". A session list has exactly the same obligation, so
//! every fetch here goes through [`RemoteServerManager::client_for_host`], which
//! is a lookup over sessions the manager already holds in `Connected` state
//! (`client_for_host` -> `client_for_session` -> `match self.sessions.get(..)`,
//! `&self`, no I/O). `None` from that lookup is the *whole* "not connected"
//! story: no request is made, no state is recorded, and the dashboard says
//! "Not connected" rather than showing a spinner that implies work in progress.
//!
//! # Nothing is cached across a disconnect
//!
//! A session list belongs to the connection it was read over. On
//! [`RemoteServerManagerEvent::HostDisconnected`] the host's entry is removed
//! outright, and a late RPC result for a host with no entry is discarded
//! instead of re-creating one ([`apply_fetch_result`]). Keeping either would
//! leave the pane rendering a list of sessions read over a connection that no
//! longer exists -- the "claim about the past" the pane's own module docs
//! reject. Nothing here is persisted, so a restart starts from "never fetched".
//!
//! # Why this is a per-pane model and not a singleton
//!
//! Every other model in this directory is a `SingletonEntity` registered in
//! `app/src/lib.rs`. This one is created by `RemoteHostsView::new` via
//! `ctx.add_model` and lives and dies with the dashboard tab, which is the
//! behaviour the "nothing is persisted" rule wants anyway: close the tab and
//! every claim it was making about another machine goes with it. Promote it to
//! a singleton only when a second surface needs the same data, and register it
//! in `lib.rs` at that point.
//!
//! # Tested without a daemon
//!
//! The decisions live in free functions over plain state -- [`apply_in_flight`],
//! [`apply_fetch_result`], [`apply_disconnect`] and [`host_sessions_display`] --
//! following `terminal/remote_server_tty/event_loop.rs`'s `outbound_rpc_for` and
//! the dashboard's own `host_install_display`. What cannot be tested here is the
//! wiring itself: whether `client_for_host` returns a client, and whether the
//! RPC round trip succeeds, are facts about a live daemon over a live SSH
//! connection. `remote_sessions_model_tests.rs` says so where it applies.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use remote_server::proto::RemoteSessionSummary;
use warp_core::HostId;
use warpui::{Entity, ModelContext, SingletonEntity};

use super::manager::{RemoteServerManager, RemoteServerManagerEvent};

/// Emitted whenever any host's fetch state changed, so the dashboard can
/// repaint. Deliberately carries no payload: the pane re-reads this model on
/// every render (the same discipline it applies to `HostRegistryModel`), so an
/// event that carried state would be a second copy of it that could disagree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteSessionsEvent {
    Changed,
}

/// What this model knows about one host's session list.
///
/// "Never fetched" is deliberately *not* a variant: it is the absence of an
/// entry in [`RemoteSessionsModel::per_host`], which is also what a host looks
/// like after [`apply_disconnect`] drops it. Both mean "this model is holding
/// nothing for that host", and giving them one representation makes it
/// impossible to leave a stale variant behind by forgetting to clear it.
#[derive(Clone, Debug, PartialEq)]
pub enum HostSessionsFetch {
    /// A `list_sessions` RPC is outstanding for this host.
    ///
    /// `superseded` records that a refresh trigger fired while this request was
    /// still in the air. Fetches are never stacked -- two concurrent
    /// `list_sessions` calls can complete out of order and let an older listing
    /// overwrite a newer one -- so a trigger that arrives mid-flight sets this
    /// flag instead, and [`apply_fetch_result`] reports it so the caller can
    /// issue exactly one follow-up fetch. Without it, an exit that lands while
    /// a fetch is in flight would leave the pane showing a session as running
    /// until the next connect.
    InFlight { superseded: bool },
    /// The daemon answered at `fetched_at` with exactly these sessions. An
    /// empty `sessions` is a real answer -- "this host is holding none" -- and
    /// is not the same statement as having no entry at all.
    Fetched {
        fetched_at: DateTime<Utc>,
        sessions: Vec<RemoteSessionSummary>,
    },
    /// The `list_sessions` RPC failed at `failed_at`. `reason` is the client
    /// error rendered as text, kept so the pane can say what went wrong instead
    /// of showing an empty list, which would read as "no sessions".
    Failed {
        failed_at: DateTime<Utc>,
        reason: String,
    },
}

/// What the dashboard should draw for one host, once connectedness is taken
/// into account.
///
/// Borrows from the model rather than cloning: the pane reads this inside
/// `render` and turns it straight into text.
#[derive(Clone, Debug, PartialEq)]
pub enum HostSessionsDisplay<'a> {
    /// No live client for this host right now. Nothing was asked, and nothing
    /// will be: see the module docs' "It never dials".
    NotConnected,
    /// Connected, but no listing has been requested or recorded yet.
    NeverFetched,
    /// A request is outstanding.
    Loading,
    /// The daemon answered. `sessions` may legitimately be empty.
    Fetched {
        fetched_at: DateTime<Utc>,
        sessions: &'a [RemoteSessionSummary],
    },
    /// The request failed.
    Failed {
        failed_at: DateTime<Utc>,
        reason: &'a str,
    },
}

/// Projects `(is this host connected right now, what do we hold for it)` onto
/// what the dashboard should say.
///
/// **Not connected wins over everything.** Even if a fetch record survived --
/// it should not, [`apply_disconnect`] removes it, but this function is the
/// last line rather than the only one -- a host with no live client renders as
/// `NotConnected` and never as a list of sessions read over a connection that
/// is gone.
///
/// `NotConnected` and `Fetched { sessions: [] }` are separate variants on
/// purpose: "we never asked" and "we asked and the host is holding none" are
/// different facts, and rendering the first as the second is the same confident
/// lie `host_install_display` exists to prevent for install state.
pub fn host_sessions_display<'a>(
    connected: bool,
    fetch: Option<&'a HostSessionsFetch>,
) -> HostSessionsDisplay<'a> {
    if !connected {
        return HostSessionsDisplay::NotConnected;
    }
    match fetch {
        None => HostSessionsDisplay::NeverFetched,
        Some(HostSessionsFetch::InFlight { .. }) => HostSessionsDisplay::Loading,
        Some(HostSessionsFetch::Fetched {
            fetched_at,
            sessions,
        }) => HostSessionsDisplay::Fetched {
            fetched_at: *fetched_at,
            sessions,
        },
        Some(HostSessionsFetch::Failed { failed_at, reason }) => HostSessionsDisplay::Failed {
            failed_at: *failed_at,
            reason,
        },
    }
}

/// Outcome of asking [`apply_in_flight`] to start a fetch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartFetch {
    /// Nothing was outstanding; the caller should issue the RPC.
    Issue,
    /// A request is already in the air. The caller must **not** issue a second
    /// one; `superseded` has been set so [`apply_fetch_result`] will ask for a
    /// follow-up when the outstanding one lands.
    AlreadyInFlight,
}

/// Records that a fetch is starting for `host_id`, and says whether the caller
/// should actually issue the RPC. Pure over the map so the no-stacking rule is
/// testable without a daemon.
pub fn apply_in_flight(
    per_host: &mut HashMap<HostId, HostSessionsFetch>,
    host_id: &HostId,
) -> StartFetch {
    if let Some(HostSessionsFetch::InFlight { superseded }) = per_host.get_mut(host_id) {
        *superseded = true;
        return StartFetch::AlreadyInFlight;
    }
    per_host.insert(
        host_id.clone(),
        HostSessionsFetch::InFlight { superseded: false },
    );
    StartFetch::Issue
}

/// What the caller should do after [`apply_fetch_result`] stored a result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AfterFetch {
    /// Result stored (or discarded); nothing further to do.
    Settled,
    /// Result stored, but a refresh trigger fired while it was in flight, so
    /// the stored listing may already be out of date. Issue one more fetch.
    Refetch,
    /// The host had no entry, so the result was discarded and nothing changed.
    /// This is the late-arrival-after-disconnect case; see the module docs.
    Discarded,
}

/// Stores the outcome of one `list_sessions` RPC.
///
/// `at` is a parameter rather than `Utc::now()` so the transition is
/// deterministic under test.
///
/// A result for a host with no entry is **discarded**. That is not defensive
/// tidiness: the only way to reach it is for `HostDisconnected` to have dropped
/// the host while its request was in the air, and re-inserting would leave the
/// pane holding a list read over a connection that has since died.
pub fn apply_fetch_result(
    per_host: &mut HashMap<HostId, HostSessionsFetch>,
    host_id: &HostId,
    result: Result<Vec<RemoteSessionSummary>, String>,
    at: DateTime<Utc>,
) -> AfterFetch {
    let Some(existing) = per_host.get(host_id) else {
        return AfterFetch::Discarded;
    };
    let superseded = matches!(existing, HostSessionsFetch::InFlight { superseded: true });
    let next = match result {
        Ok(sessions) => HostSessionsFetch::Fetched {
            fetched_at: at,
            sessions,
        },
        Err(reason) => HostSessionsFetch::Failed {
            failed_at: at,
            reason,
        },
    };
    per_host.insert(host_id.clone(), next);
    if superseded {
        AfterFetch::Refetch
    } else {
        AfterFetch::Settled
    }
}

/// Drops everything held for `host_id`, returning whether anything was there.
/// See the module docs' "Nothing is cached across a disconnect".
pub fn apply_disconnect(
    per_host: &mut HashMap<HostId, HostSessionsFetch>,
    host_id: &HostId,
) -> bool {
    per_host.remove(host_id).is_some()
}

/// The dashboard's view of which hosts are holding pty sessions.
#[derive(Default)]
pub struct RemoteSessionsModel {
    per_host: HashMap<HostId, HostSessionsFetch>,
}

impl Entity for RemoteSessionsModel {
    type Event = RemoteSessionsEvent;
}

impl RemoteSessionsModel {
    /// Subscribes to the manager's event stream. Does **not** fetch anything:
    /// the pane calls [`Self::fetch_for_connected_hosts`] once it is open, the
    /// same split `RemoteHostsView::new` already uses for the registry.
    pub fn new(ctx: &mut ModelContext<Self>) -> Self {
        let manager = RemoteServerManager::handle(ctx);
        ctx.subscribe_to_model(&manager, |me, event, ctx| {
            me.handle_manager_event(event, ctx);
        });
        Self::default()
    }

    /// What this model holds for `host_id`, if anything. `None` means "never
    /// fetched, or dropped on disconnect" -- feed it to [`host_sessions_display`]
    /// along with whether the host is connected to get something renderable.
    pub fn fetch_state(&self, host_id: &HostId) -> Option<&HostSessionsFetch> {
        self.per_host.get(host_id)
    }

    /// The set of hosts with a live client right now, straight from the
    /// manager's in-memory session map. No I/O, and in particular no dial --
    /// this is the same `connected_host_ids` read
    /// `HostRegistryModel::refresh_from_live_connections` uses.
    pub fn connected_host_ids(ctx: &warpui::AppContext) -> HashSet<HostId> {
        RemoteServerManager::as_ref(ctx)
            .connected_host_ids()
            .cloned()
            .collect()
    }

    /// Fetches session lists for every host that **already** has a live
    /// connection. Called on pane open. Hosts with no live client are skipped
    /// entirely: see the module docs' "It never dials".
    pub fn fetch_for_connected_hosts(&mut self, ctx: &mut ModelContext<Self>) {
        // Collected into a local first: `connected_host_ids` borrows `ctx`, and
        // `start_fetch` needs it mutably, so the read has to finish before the
        // loop body begins.
        let host_ids = Self::connected_host_ids(ctx);
        for host_id in host_ids {
            self.start_fetch(host_id, ctx);
        }
    }

    /// Issues one `list_sessions` for `host_id`, if it has a live client and
    /// nothing is already in flight for it.
    fn start_fetch(&mut self, host_id: HostId, ctx: &mut ModelContext<Self>) {
        // The no-dial guarantee, in one line: a lookup over already-`Connected`
        // sessions, returning `None` for anything else. Nothing below this
        // point runs for a host that is not already connected.
        let client = RemoteServerManager::as_ref(ctx)
            .client_for_host(&host_id)
            .cloned();
        let Some(client) = client else {
            if apply_disconnect(&mut self.per_host, &host_id) {
                ctx.emit(RemoteSessionsEvent::Changed);
            }
            return;
        };

        if apply_in_flight(&mut self.per_host, &host_id) == StartFetch::AlreadyInFlight {
            return;
        }
        ctx.emit(RemoteSessionsEvent::Changed);

        // `ClientError` is flattened to a string inside the future: the spawned
        // output must be `Send`, and the pane only ever renders this as text.
        // `send_request` applies `REQUEST_TIMEOUT`, so this future always
        // resolves rather than leaving the host stuck on "Checking".
        ctx.spawn(
            async move {
                client
                    .list_sessions()
                    .await
                    .map_err(|error| format!("{error}"))
            },
            move |me, result, ctx| {
                me.record_fetch_result(host_id, result, ctx);
            },
        );
    }

    fn record_fetch_result(
        &mut self,
        host_id: HostId,
        result: Result<Vec<RemoteSessionSummary>, String>,
        ctx: &mut ModelContext<Self>,
    ) {
        match apply_fetch_result(&mut self.per_host, &host_id, result, Utc::now()) {
            AfterFetch::Discarded => {}
            AfterFetch::Settled => ctx.emit(RemoteSessionsEvent::Changed),
            AfterFetch::Refetch => {
                ctx.emit(RemoteSessionsEvent::Changed);
                self.start_fetch(host_id, ctx);
            }
        }
    }

    /// The match is exhaustive, and the "not ours" arm is spelled out rather
    /// than left to a wildcard, for the reason `codebase_index_model.rs` and
    /// `terminal/remote_server_tty/event_loop.rs` both give: a wildcard would
    /// silently swallow a future host- or session-addressed event this model
    /// ought to refetch on.
    fn handle_manager_event(
        &mut self,
        event: &RemoteServerManagerEvent,
        ctx: &mut ModelContext<Self>,
    ) {
        match event {
            // A host just gained its first live connection, so it now has a
            // client to ask -- and asking costs no new dial.
            RemoteServerManagerEvent::HostConnected { host_id } => {
                self.start_fetch(host_id.clone(), ctx);
            }
            // The last connection to this host is gone. Drop the listing rather
            // than keep showing it.
            RemoteServerManagerEvent::HostDisconnected { host_id } => {
                if apply_disconnect(&mut self.per_host, host_id) {
                    ctx.emit(RemoteSessionsEvent::Changed);
                }
            }
            // A pty session on this host just exited, which is exactly the
            // running/exited distinction this dashboard draws, so the listing we
            // hold is now wrong. `start_fetch` re-checks `client_for_host`, so
            // an exit pushed as the connection is tearing down still does not
            // dial.
            RemoteServerManagerEvent::SessionExited { host_id, .. } => {
                self.start_fetch(host_id.clone(), ctx);
            }
            // Deliberately ignored: `SessionDeregistered` carries only a
            // `SessionId` (a client SSH connection, not a pty session) and no
            // `host_id`, and by the time it fires the session has already been
            // removed from the manager's map -- so there is nothing left to
            // resolve it against. The case that matters for this model, the
            // last connection to a host going away, arrives as
            // `HostDisconnected` above.
            RemoteServerManagerEvent::SessionDeregistered { .. } => {}
            RemoteServerManagerEvent::SessionConnecting { .. }
            | RemoteServerManagerEvent::SessionConnected { .. }
            | RemoteServerManagerEvent::SessionConnectionFailed { .. }
            | RemoteServerManagerEvent::SessionDisconnected { .. }
            | RemoteServerManagerEvent::SessionReconnected { .. }
            | RemoteServerManagerEvent::RemoteAgentContextSnapshot { .. }
            | RemoteServerManagerEvent::NavigatedToDirectory { .. }
            | RemoteServerManagerEvent::RepoMetadataSnapshot { .. }
            | RemoteServerManagerEvent::RepoMetadataUpdated { .. }
            | RemoteServerManagerEvent::RepoMetadataDirectoryLoaded { .. }
            | RemoteServerManagerEvent::BufferUpdated { .. }
            | RemoteServerManagerEvent::BufferConflictDetected { .. }
            | RemoteServerManagerEvent::SessionOutputChunk { .. }
            | RemoteServerManagerEvent::DiffStateSnapshotReceived { .. }
            | RemoteServerManagerEvent::DiffStateMetadataUpdateReceived { .. }
            | RemoteServerManagerEvent::DiffStateFileDeltaReceived { .. }
            | RemoteServerManagerEvent::GitStatusPushReceived { .. }
            | RemoteServerManagerEvent::GitHubPrInfoPushReceived { .. }
            | RemoteServerManagerEvent::GitHubRepositoryInfoPushReceived { .. }
            | RemoteServerManagerEvent::CodebaseIndexStatusesSnapshot { .. }
            | RemoteServerManagerEvent::CodebaseIndexStatusUpdated { .. }
            | RemoteServerManagerEvent::CodebaseIndexMutationFailed { .. }
            | RemoteServerManagerEvent::SetupStateChanged { .. }
            | RemoteServerManagerEvent::BinaryCheckComplete { .. }
            | RemoteServerManagerEvent::BinaryInstallComplete { .. }
            | RemoteServerManagerEvent::ClientRequestFailed { .. }
            | RemoteServerManagerEvent::ServerMessageDecodingError { .. } => {}
        }
    }
}

#[cfg(test)]
#[path = "remote_sessions_model_tests.rs"]
mod tests;
