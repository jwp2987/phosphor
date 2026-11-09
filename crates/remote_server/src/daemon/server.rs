//! The helper daemon's single-owner core: one thread owns all state and is driven by an mpsc of
//! connection events. Blocking work runs on worker threads ([`super::jobs`]) that reply
//! directly on the originating connection.
//!
//! This is the warpui-free counterpart of the app daemon's `ServerModel`. It implements the
//! request families listed in [`crate::capability::HELPER_CAPABILITIES`]; every other request
//! is answered with `ErrorCode::UNSUPPORTED` so the client takes its no-helper path.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use crate::capability::HELPER_CAPABILITIES;
use crate::proto::{
    ClientMessage, ErrorCode, Initialize, InitializeResponse, ServerMessage, SessionBootstrapped,
    client_message, host_scoped_request, notification, server_message, session_scoped_request,
};
use crate::protocol::RequestId;

use super::exec::{self, SessionShell, ShellKind};
use super::jobs::{Inflight, Reply};
use super::watch::{FsKind, WatchSet};
use super::{
    buffers, diff_state, file_context, fs_ops, git, git_status, git_write, github, repo_meta,
    ripgrep,
};

/// How long the daemon waits with no connections before exiting.
pub(super) const GRACE_PERIOD: Duration = Duration::from_secs(10 * 60);

/// Unique identifier for a connected proxy session.
pub(super) type ConnId = u64;

static NEXT_CONN_ID: AtomicU64 = AtomicU64::new(1);

pub(super) fn next_conn_id() -> ConnId {
    NEXT_CONN_ID.fetch_add(1, Ordering::Relaxed)
}

/// What the connection threads and the accept loop tell the core.
pub(super) enum Event {
    Connected {
        conn: ConnId,
        tx: async_channel::Sender<ServerMessage>,
        chunking: Arc<AtomicBool>,
    },
    Message {
        conn: ConnId,
        msg: ClientMessage,
    },
    Disconnected {
        conn: ConnId,
    },
    /// The filesystem watcher saw a change (`daemon/watch.rs`). No debounce: one event per OS
    /// event; consumers coalesce with their own dirty flag.
    FsChange {
        paths: Vec<PathBuf>,
        kind: FsKind,
    },
    /// A worker finished walking a repository for `NavigatedToDirectory`.
    RepoIndexed(Box<repo_meta::Indexed>),
    /// A worker finished re-listing the directories a watcher event dirtied.
    RepoRelisted(repo_meta::Relisted),
}

struct Connection {
    tx: async_channel::Sender<ServerMessage>,
    chunking: Arc<AtomicBool>,
}

/// Sends unsolicited pushes (no request id) to a connection. Handlers get one so they can push
/// without owning the connection table.
pub(super) struct Pusher<'a> {
    conns: &'a HashMap<ConnId, Connection>,
}

impl Pusher<'_> {
    /// Queues `message` for `conn`'s writer, after anything already queued for it (so a push
    /// sent right after a response arrives right after it). A gone connection drops it.
    pub(super) fn send(&self, conn: ConnId, message: server_message::Message) {
        if let Some(c) = self.conns.get(&conn) {
            let _ = c.tx.try_send(ServerMessage {
                request_id: String::new(),
                message: Some(message),
            });
        }
    }
}

/// Whether this daemon splits large responses into `FrameChunk`s for a client that asked
/// (#1244). `PHOSPHOR_REMOTE_SERVER_CHUNKED_RESPONSES=0` in the daemon's environment turns it
/// off for every connection; the client then gets whole frames, as from an older daemon.
fn chunked_responses_for(client_asked: bool) -> bool {
    client_asked
        && chunked_responses_enabled_from_env(
            std::env::var("PHOSPHOR_REMOTE_SERVER_CHUNKED_RESPONSES")
                .ok()
                .as_deref(),
        )
}

pub(super) fn chunked_responses_enabled_from_env(value: Option<&str>) -> bool {
    !matches!(value.map(str::trim), Some("0" | "false" | "off" | "no"))
}

pub(super) struct Server {
    conns: HashMap<ConnId, Connection>,
    /// Shells by the session id of `SessionBootstrapped`.
    shells: HashMap<u64, SessionShell>,
    inflight: Inflight,
    host_id: String,
    /// Kept for parity with the app daemon; the helper makes no authenticated requests.
    auth_token: Option<String>,
    idle_since: Option<Instant>,
    grace: Duration,
    /// The core's own sender, for workers and the watcher to report back. `None` in unit tests
    /// that drive the core by hand; watching is then off.
    events_tx: Option<std::sync::mpsc::Sender<Event>>,
    /// The one refcounted watch set every consumer shares (see `watch.rs` for the API).
    watches: WatchSet,
    /// Indexed repository roots and their live trees (`repo_meta.rs`).
    repos: repo_meta::RepoTrees,
}

impl Server {
    pub(super) fn new(grace: Duration) -> Self {
        let host_id = uuid::Uuid::new_v4().to_string();
        log::info!(
            "Daemon started: PID={}, host_id={host_id}, version={}",
            std::process::id(),
            crate::naming::helper_version()
        );
        Self {
            conns: HashMap::new(),
            shells: HashMap::new(),
            inflight: Inflight::default(),
            host_id,
            auth_token: None,
            idle_since: Some(Instant::now()),
            grace,
            events_tx: None,
            watches: WatchSet::disabled(),
            repos: repo_meta::RepoTrees::default(),
        }
    }

    /// Gives the core a sender to its own event channel, which turns filesystem watching on and
    /// lets worker threads report back.
    pub(super) fn with_events(mut self, events: std::sync::mpsc::Sender<Event>) -> Self {
        self.watches = WatchSet::new(events.clone());
        self.events_tx = Some(events);
        self
    }

    /// Runs until the event channel closes or the daemon has been idle for the grace period.
    pub(super) fn run(&mut self, events: Receiver<Event>) {
        loop {
            let wait = match self.idle_since {
                Some(since) => self.grace.saturating_sub(since.elapsed()),
                // Connected: wake rarely; only an event matters.
                None => Duration::from_secs(3600),
            };
            match events.recv_timeout(wait) {
                Ok(event) => self.handle_event(event),
                Err(RecvTimeoutError::Timeout) => {
                    if self.conns.is_empty()
                        && self
                            .idle_since
                            .is_some_and(|since| since.elapsed() >= self.grace)
                    {
                        log::info!("Daemon: idle for {:?}, exiting", self.grace);
                        return;
                    }
                }
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    pub(super) fn handle_event(&mut self, event: Event) {
        match event {
            Event::Connected { conn, tx, chunking } => {
                self.conns.insert(conn, Connection { tx, chunking });
                self.idle_since = None;
                log::info!(
                    "Daemon: connection {conn} registered, {} active, host_id={}",
                    self.conns.len(),
                    self.host_id
                );
            }
            Event::Message { conn, msg } => self.handle_message(conn, msg),
            Event::FsChange { paths, kind } => self.on_fs_change(&paths, kind),
            Event::RepoIndexed(done) => {
                let push = Pusher { conns: &self.conns };
                self.repos.indexed(*done, &mut self.watches, &push);
            }
            Event::RepoRelisted(done) => {
                let push = Pusher { conns: &self.conns };
                self.repos
                    .relisted(done, self.events_tx.as_ref(), &mut self.watches, &push);
            }
            Event::Disconnected { conn } => {
                self.conns.remove(&conn);
                self.repos.conn_gone(conn, &mut self.watches);
                log::info!(
                    "Daemon: connection {conn} gone, {} active",
                    self.conns.len()
                );
                if self.conns.is_empty() {
                    self.idle_since = Some(Instant::now());
                }
            }
        }
    }

    /// Fans a filesystem change out to every module that watches paths. Add one line per
    /// consumer here (the repo tree, later buffers, diff state and skills).
    fn on_fs_change(&mut self, paths: &[PathBuf], kind: FsKind) {
        log::trace!("Daemon: fs change {kind:?} {} path(s)", paths.len());
        self.repos
            .on_fs_change(paths, kind, self.events_tx.as_ref());
    }

    fn reply_for(&self, conn: ConnId, request_id: &RequestId) -> Option<Reply> {
        self.conns
            .get(&conn)
            .map(|c| Reply::new(c.tx.clone(), request_id.clone().into()))
    }

    pub(super) fn handle_message(&mut self, conn: ConnId, msg: ClientMessage) {
        let request_id = RequestId::from(msg.request_id);
        let Some(reply) = self.reply_for(conn, &request_id) else {
            log::debug!("Daemon: message for unknown conn {conn}");
            return;
        };
        match msg.message {
            Some(client_message::Message::HostScoped(req)) => match req.message {
                Some(message) => self.handle_host_scoped(message, reply),
                None => reply.error(
                    ErrorCode::InvalidRequest,
                    "HostScopedRequest had no message variant set",
                ),
            },
            Some(client_message::Message::SessionScoped(req)) => match req.message {
                Some(message) => self.handle_session_scoped(conn, message, reply),
                None => reply.error(
                    ErrorCode::InvalidRequest,
                    "SessionScopedRequest had no message variant set",
                ),
            },
            Some(client_message::Message::Notification(n)) => {
                self.handle_notification(n.message, &request_id)
            }
            None => reply.error(
                ErrorCode::InvalidRequest,
                "ClientMessage had no message variant set",
            ),
        }
    }

    fn handle_host_scoped(&mut self, message: host_scoped_request::Message, reply: Reply) {
        use host_scoped_request::Message as M;
        match message {
            M::WriteFile(msg) => reply.send(fs_ops::write_file(msg)),
            M::DeleteFile(msg) => reply.send(fs_ops::delete_file(msg)),
            M::ListDirectory(msg) => reply.send(fs_ops::list_directory(msg)),
            M::ResolvePath(msg) => reply.send(fs_ops::resolve_path(msg)),
            M::CreateDirectory(msg) => reply.send(fs_ops::create_directory(msg)),
            M::ReadFileChunk(msg) => reply.send(fs_ops::read_file_chunk(msg)),
            M::WriteFileChunk(msg) => reply.send(fs_ops::write_file_chunk(msg)),
            M::GetBranches(msg) => {
                log::info!("Handling GetBranches repo={}", msg.repo_path);
                self.inflight.spawn(reply, move |_| {
                    server_message::Message::GetBranchesResponse(git::handle_get_branches(msg))
                });
            }
            M::GetCommittedBranchFiles(msg) => {
                log::info!("Handling GetCommittedBranchFiles repo={}", msg.repo_path);
                self.inflight.spawn(reply, move |_| {
                    server_message::Message::GetCommittedBranchFilesResponse(
                        git::handle_get_committed_branch_files(msg),
                    )
                });
            }
            M::RipgrepSearch(msg) => {
                log::info!("Handling RipgrepSearch ({} roots)", msg.roots.len());
                match ripgrep::validate_request(msg) {
                    Err(message) => reply.send(server_message::Message::RipgrepSearchResponse(
                        ripgrep::error_response(message),
                    )),
                    Ok(params) => self.inflight.spawn(reply, move |cancel| {
                        server_message::Message::RipgrepSearchResponse(
                            ripgrep::search_result_to_response(ripgrep::run_search(params, cancel)),
                        )
                    }),
                }
            }
            // Reserved family modules (P0): each answers UNSUPPORTED until its package lands.
            M::ReadFileContext(request) => self.inflight.spawn(reply, move |cancel| {
                server_message::Message::ReadFileContextResponse(file_context::read_file_context(
                    &request, cancel,
                ))
            }),
            M::SaveBuffer(_) => buffers::handle(&reply, "SaveBuffer"),
            M::ResolveConflict(_) => buffers::handle(&reply, "ResolveConflict"),
            M::GitCommitChain(msg) => self
                .inflight
                .spawn(reply, move |_| git_write::commit_chain(msg)),
            M::GitPush(msg) => self.inflight.spawn(reply, move |_| git_write::push(msg)),
            M::GitPull(msg) => self.inflight.spawn(reply, move |_| git_write::pull(msg)),
            M::GitCreatePr(msg) => self
                .inflight
                .spawn(reply, move |_| git_write::create_pr(msg)),
            M::DiscardFiles(msg) => self
                .inflight
                .spawn(reply, move |_| git_write::discard_files(msg)),
            M::GitStage(msg) => self.inflight.spawn(reply, move |_| git_write::stage(msg)),
            // Legacy host-side indexing: never implemented by the helper (see CODEBASE_CHUNKS).
            other => unsupported(&reply, host_scoped_name(&other)),
        }
    }

    fn handle_session_scoped(
        &mut self,
        conn: ConnId,
        message: session_scoped_request::Message,
        reply: Reply,
    ) {
        use session_scoped_request::Message as M;
        match message {
            M::Initialize(msg) => self.handle_initialize(conn, msg, &reply),
            M::RunCommand(req) => {
                log::info!(
                    "Handling RunCommand (session_id={}, cwd={:?})",
                    req.session_id,
                    req.working_directory
                );
                let Some(shell) = self.shells.get(&req.session_id).cloned() else {
                    log::error!(
                        "No executor for session {}, session was never initialized",
                        req.session_id
                    );
                    reply.send(exec::session_not_found(req.session_id));
                    return;
                };
                self.inflight
                    .spawn(reply, move |cancel| exec::run_command(&shell, req, cancel));
            }
            M::NavigatedToDirectory(msg) => {
                let push = Pusher { conns: &self.conns };
                self.repos.navigate(
                    conn,
                    msg,
                    reply,
                    self.events_tx.as_ref(),
                    &mut self.watches,
                    &push,
                );
            }
            M::LoadRepoMetadataDirectory(msg) => {
                self.repos.load_directory(msg, &reply, &mut self.watches)
            }
            M::OpenBuffer(_) => buffers::handle(&reply, "OpenBuffer"),
            M::GetDiffState(_) => diff_state::handle(&reply, "GetDiffState"),
        }
    }

    fn handle_initialize(&mut self, conn: ConnId, msg: Initialize, reply: &Reply) {
        log::info!("Handling Initialize (conn={conn})");
        if !msg.auth_token.is_empty() {
            self.auth_token = Some(msg.auth_token);
        }
        let chunked = chunked_responses_for(msg.chunked_responses);
        // From here on the writer may split this connection's large responses.
        if let Some(c) = self.conns.get(&conn) {
            c.chunking.store(chunked, Ordering::Release);
        }
        reply.send(server_message::Message::InitializeResponse(
            InitializeResponse {
                server_version: crate::naming::helper_version().to_string(),
                host_id: self.host_id.clone(),
                chunked_responses: chunked,
                capabilities: HELPER_CAPABILITIES.iter().map(|c| c.to_string()).collect(),
            },
        ));
    }

    fn handle_notification(
        &mut self,
        message: Option<notification::Message>,
        request_id: &RequestId,
    ) {
        use notification::Message as N;
        match message {
            Some(N::Abort(abort)) => {
                let aborted = self.inflight.abort(&abort.request_id_to_abort);
                log::info!(
                    "Handling Abort of {} ({})",
                    abort.request_id_to_abort,
                    if aborted {
                        "cancelled"
                    } else {
                        "not in flight"
                    }
                );
            }
            Some(N::Authenticate(msg)) => {
                if !msg.auth_token.is_empty() {
                    self.auth_token = Some(msg.auth_token);
                }
            }
            Some(N::SessionBootstrapped(msg)) => self.handle_session_bootstrapped(msg),
            Some(N::UpdateGitStatus(_)) => git_status::ignore("UpdateGitStatus"),
            Some(N::UpdateGithubPrInfo(_)) => github::ignore("UpdateGitHubPrInfo"),
            Some(N::UpdateGithubRepoInfo(_)) => github::ignore("UpdateGitHubRepoInfo"),
            // Notifications carry no response contract. The ones the helper cannot act on
            // (buffer sync, diff-state and git-status subscriptions, preferences) are dropped;
            // the client does not wait on a response, it waits on pushes that never come, and
            // those features run on the no-helper path because the capability list says so.
            Some(other) => log::debug!(
                "Daemon: ignoring unsupported notification {} (request_id={request_id})",
                notification_name(&other)
            ),
            None => log::warn!(
                "Received Notification with no message variant (request_id={request_id})"
            ),
        }
    }

    fn handle_session_bootstrapped(&mut self, msg: SessionBootstrapped) {
        log::info!(
            "Handling SessionBootstrapped: session_id={}, shell_type={:?}, shell_path={:?}",
            msg.session_id,
            msg.shell_type,
            msg.shell_path
        );
        let Some(kind) = ShellKind::from_name(&msg.shell_type) else {
            log::error!(
                "Unknown shell_type {:?} in SessionBootstrapped for session {}",
                msg.shell_type,
                msg.session_id
            );
            return;
        };
        let path = msg.shell_path.map(std::path::PathBuf::from);
        if path.is_none() {
            log::warn!(
                "SessionBootstrapped for session {} had no shell_path; falling back to the bare shell name",
                msg.session_id
            );
        }
        if self
            .shells
            .insert(msg.session_id, SessionShell { kind, path })
            .is_some()
        {
            log::warn!(
                "Overwriting existing executor for session {} (re-SessionBootstrapped)",
                msg.session_id
            );
        }
    }
}

pub(super) fn unsupported(reply: &Reply, what: &str) {
    reply.error(
        ErrorCode::Unsupported,
        format!("phosphor-remote does not support {what}; the client uses its own path"),
    );
}

fn host_scoped_name(message: &host_scoped_request::Message) -> &'static str {
    use host_scoped_request::Message as M;
    match message {
        M::WriteFile(_) => "WriteFile",
        M::DeleteFile(_) => "DeleteFile",
        M::ReadFileContext(_) => "ReadFileContext",
        M::SaveBuffer(_) => "SaveBuffer",
        M::ResolveConflict(_) => "ResolveConflict",
        M::GetBranches(_) => "GetBranches",
        M::GitCommitChain(_) => "GitCommitChain",
        M::GitPush(_) => "GitPush",
        M::GitCreatePr(_) => "GitCreatePr",
        M::GetCommittedBranchFiles(_) => "GetCommittedBranchFiles",
        M::RipgrepSearch(_) => "RipgrepSearch",
        M::ListDirectory(_) => "ListDirectory",
        M::ResolvePath(_) => "ResolvePath",
        M::CreateDirectory(_) => "CreateDirectory",
        M::ReadFileChunk(_) => "ReadFileChunk",
        M::WriteFileChunk(_) => "WriteFileChunk",
        M::DiscardFiles(_) => "DiscardFiles",
        M::IndexCodebase(_) => "IndexCodebase",
        M::DropCodebaseIndex(_) => "DropCodebaseIndex",
        M::GetFragmentMetadataFromHash(_) => "GetFragmentMetadataFromHash",
        M::ResyncCodebase(_) => "ResyncCodebase",
        M::SearchRemoteCodebase(_) => "SearchRemoteCodebase",
        M::GitPull(_) => "GitPull",
        M::GitStage(_) => "GitStage",
    }
}

fn notification_name(message: &notification::Message) -> &'static str {
    use notification::Message as N;
    match message {
        N::Abort(_) => "Abort",
        N::Authenticate(_) => "Authenticate",
        N::SessionBootstrapped(_) => "SessionBootstrapped",
        N::BufferEdit(_) => "BufferEdit",
        N::CloseBuffer(_) => "CloseBuffer",
        N::UnsubscribeDiffState(_) => "UnsubscribeDiffState",
        N::UpdatePreferences(_) => "UpdatePreferences",
        N::UpdateGitStatus(_) => "UpdateGitStatus",
        N::UpdateGithubPrInfo(_) => "UpdateGitHubPrInfo",
        N::UpdateGithubRepoInfo(_) => "UpdateGitHubRepoInfo",
    }
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
