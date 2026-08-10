//! The consumer for the codebase embedding index.
//!
//! # What this is, and why it exists
//!
//! [`CodebaseIndexManager::retrieve_relevant_files`] is the only way to ask the
//! embedding index a question. Until this module it had **zero callers** — the app
//! embedded the whole repository against the user's own `/embeddings` endpoint, grew a
//! vector store, kept it in sync, and then never asked it anything. Enabling
//! *Settings > Code > codebase context* cost real money for an answer nothing read.
//!
//! # Correspondence to the pin
//!
//! The pin drives retrieval from `app/src/ai/get_relevant_files/controller.rs`
//! (`GetRelevantFilesController`), a per-session model that owns the retrieval-id /
//! abort lifecycle: `send_local_request` starts a retrieval and records its
//! `RetrievalID`, a `CodebaseIndexManagerEvent` subscription maps completion events
//! back to the originating agent action, and `cancel_request_for_action` aborts. That
//! directory does not exist in this fork; it was retired with the inherited outline
//! removal (see the note at `app/src/ai/blocklist/block.rs:1119`).
//!
//! This module is that lifecycle, re-homed. It keeps the pin's structure — one model,
//! one subscription, a map of in-flight `RetrievalID`s, abort on supersede — and
//! changes only what the fork's architecture forces:
//!
//! * **Keyed by repository, not by agent action.** The pin keys pending requests by
//!   `AIAgentActionId` because retrieval backs a `SearchCodebase` *action* routed
//!   through `BlocklistAIActionModel`. This fork has no such action: `search_codebase`
//!   and `get_relevant_files` are intercepted by name in `chat_stream` and executed
//!   locally. There is no action id to key on, so requests are keyed by repository
//!   root, which is also the unit a newer query supersedes.
//! * **Answered through a channel, not an event.** The pin emits
//!   `GetRelevantFilesControllerEvent::Success` and a view subscribes. This fork's tool
//!   interceptor is a plain `async fn` with no `AppContext` and nothing to subscribe
//!   with, so each request carries a `oneshot::Sender` and the caller awaits it.
//! * **No telemetry.** The pin sends `FullEmbedCodebaseContextSearch{Success,Failed}`;
//!   that telemetry is part of the dropped cloud surface.
//!
//! # How a query reaches the model
//!
//! [`CodebaseRetrievalHandle`] is the piece that crosses the thread boundary. It wraps
//! a [`ModelSpawner`], which schedules a closure back onto the controller *on the main
//! thread* and returns its value to the awaiting background task — so the async tool
//! interceptor can drive a `&mut ModelContext` operation without holding one. The
//! spawner is created **once**, in [`CodebaseRetrievalController::new`], and cloned per
//! request: `ModelContext::spawner()` registers a task callback that lives as long as
//! the model, so calling it per request would leak one callback per request.
//!
//! Nothing here blocks the agent's turn. Retrieval starts when the model *calls*
//! `get_relevant_files`, which is a tool call — the turn is already suspended waiting
//! for a tool result. No request is issued for a turn where the model never asks.
//!
//! # Degrading with no embedding provider
//!
//! The default configuration in a BYOP fork has no embedding provider at all, so this
//! path must be silent, cheap and correct rather than merely non-fatal:
//!
//! * With indexing off, or no index for this directory, [`handle_for_directory`]
//!   returns `None` before any work happens and the tool never consults the index.
//! * With indexing on but no provider, syncing fails, so the index has no synced root
//!   node and `retrieve_relevant_files` returns `RetrieveFileError::IndexFailed`
//!   **synchronously** — no HTTP request, no embedding spend, no waiting.
//! * Every such outcome is a [`RetrievalFailure`] the tool reports as an ordinary
//!   status, logged at `debug` only. An unconfigured provider is the user's
//!   configuration, not a defect, so it must not `report_error!` or log at `warn`
//!   on a path that now runs on ordinary agent queries.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ai::index::full_source_code_embedding::RetrievalID;
use ai::index::full_source_code_embedding::manager::RetrieveFileError;
#[cfg(feature = "local_fs")]
use ai::index::full_source_code_embedding::manager::{
    CodebaseIndexManager, CodebaseIndexManagerEvent,
};
use futures::channel::oneshot;
use warpui::{AppContext, Entity, ModelContext, ModelSpawner, SingletonEntity};

/// Why a retrieval produced nothing.
///
/// Every variant is an ordinary, expected state in a fork where the user brings the
/// provider — none of them is a defect, and none should be reported as one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetrievalFailure {
    /// No embedding index exists for this repository: indexing is off, the repository
    /// was never indexed, or the launch mode does not index.
    NoIndex,
    /// The index is still being built. Common on the first query after opening a
    /// repository; the next turn will usually succeed.
    Syncing,
    /// An index exists but its last sync failed. **This is the no-provider case**: with
    /// no embedding provider configured, syncing fails with `NoEmbeddingProvider` and
    /// the index never acquires a synced root node.
    IndexUnavailable(String),
    /// The retrieval itself failed after starting.
    Failed(String),
    /// A newer query for the same repository replaced this one.
    Superseded,
    /// The controller went away (the app is shutting down).
    Unavailable,
}

impl RetrievalFailure {
    /// A short, stable token naming this outcome, for the tool result the model sees.
    pub fn status(&self) -> &'static str {
        match self {
            Self::NoIndex => "no_index",
            Self::Syncing => "index_syncing",
            Self::IndexUnavailable(_) => "index_unavailable",
            Self::Failed(_) => "retrieval_failed",
            Self::Superseded => "superseded",
            Self::Unavailable => "unavailable",
        }
    }
}

impl From<RetrieveFileError> for RetrievalFailure {
    fn from(error: RetrieveFileError) -> Self {
        match error {
            RetrieveFileError::IndexNotFound => Self::NoIndex,
            RetrieveFileError::IndexSyncing => Self::Syncing,
            RetrieveFileError::IndexFailed(error) => Self::IndexUnavailable(error.to_string()),
        }
    }
}

/// What a retrieval produced: the matching files in rank order, best first.
///
/// Paths are absolute, as the index stores them.
pub type RetrievalResult = Result<Arc<Vec<PathBuf>>, RetrievalFailure>;

/// A retrieval in flight, and who is waiting for it.
struct PendingRetrieval {
    repo_root: PathBuf,
    responder: oneshot::Sender<RetrievalResult>,
}

/// Owns the retrieval-id / abort lifecycle for the codebase embedding index.
///
/// Registered as a singleton in `lib.rs`. See the module documentation for how this
/// corresponds to the pin's `GetRelevantFilesController`.
pub struct CodebaseRetrievalController {
    /// Created once, cloned per request. See the module documentation.
    spawner: ModelSpawner<Self>,
    /// Retrievals started and not yet resolved, keyed by the id the index gave us —
    /// the only thing completion events carry.
    pending: HashMap<RetrievalID, PendingRetrieval>,
    /// The retrieval currently in flight for each repository, so a newer query can
    /// supersede it. At most one per repository: the agent only ever consumes the
    /// newest answer, and leaving the previous one running spends the user's
    /// embedding budget on a result nobody reads.
    in_flight_by_repo: HashMap<PathBuf, RetrievalID>,
}

impl CodebaseRetrievalController {
    pub fn new(ctx: &mut ModelContext<Self>) -> Self {
        let spawner = ctx.spawner();
        Self::subscribe_to_index_manager(ctx);
        Self {
            spawner,
            pending: HashMap::new(),
            in_flight_by_repo: HashMap::new(),
        }
    }

    /// Only the `local_fs` build registers a `CodebaseIndexManager`, and
    /// `SingletonEntity::handle` panics on an unregistered singleton. This controller
    /// is registered unconditionally so [`handle_for_directory`] never has to be cfg'd
    /// at its call site; without the manager it has nothing to subscribe to and
    /// reports `NoIndex` for everything.
    #[cfg(feature = "local_fs")]
    fn subscribe_to_index_manager(ctx: &mut ModelContext<Self>) {
        let manager = CodebaseIndexManager::handle(ctx);
        ctx.subscribe_to_model(&manager, Self::handle_index_manager_event);
    }

    #[cfg(not(feature = "local_fs"))]
    fn subscribe_to_index_manager(_ctx: &mut ModelContext<Self>) {}

    /// Starts a retrieval for `repo_root`, superseding any retrieval already in flight
    /// for that repository.
    ///
    /// Returns the channel the answer will arrive on, or the reason no retrieval could
    /// be started. Errors here are synchronous and free: nothing has been sent to any
    /// provider at the point this returns `Err`.
    fn start_retrieval(
        &mut self,
        repo_root: PathBuf,
        query: String,
        ctx: &mut ModelContext<Self>,
    ) -> Result<oneshot::Receiver<RetrievalResult>, RetrievalFailure> {
        self.supersede_in_flight(&repo_root, ctx);

        let retrieval_id = Self::begin_index_retrieval(repo_root.as_path(), query, ctx)?;

        let (responder, receiver) = oneshot::channel();
        self.pending.insert(
            retrieval_id.clone(),
            PendingRetrieval {
                repo_root: repo_root.clone(),
                responder,
            },
        );
        self.in_flight_by_repo.insert(repo_root, retrieval_id);
        Ok(receiver)
    }

    /// The one call into the embedding index. Synchronous: it registers the request
    /// and returns its id; the answer arrives later as a `CodebaseIndexManagerEvent`.
    #[cfg(feature = "local_fs")]
    fn begin_index_retrieval(
        repo_root: &Path,
        query: String,
        ctx: &mut ModelContext<Self>,
    ) -> Result<RetrievalID, RetrievalFailure> {
        let manager = CodebaseIndexManager::handle(ctx);
        manager
            .update(ctx, |manager, ctx| {
                manager.retrieve_relevant_files(query, repo_root, ctx)
            })
            .map_err(RetrievalFailure::from)
    }

    #[cfg(not(feature = "local_fs"))]
    fn begin_index_retrieval(
        _repo_root: &Path,
        _query: String,
        _ctx: &mut ModelContext<Self>,
    ) -> Result<RetrievalID, RetrievalFailure> {
        Err(RetrievalFailure::NoIndex)
    }

    /// Cancels the retrieval in flight for `repo_root`, if any, and tells whoever was
    /// waiting that it was superseded.
    fn supersede_in_flight(&mut self, repo_root: &Path, ctx: &mut ModelContext<Self>) {
        let Some(retrieval_id) = self.in_flight_by_repo.remove(repo_root) else {
            return;
        };
        if let Some(pending) = self.pending.remove(&retrieval_id) {
            // The receiver is usually gone already (the superseded tool call
            // finished); send is best-effort either way.
            let _ = pending.responder.send(Err(RetrievalFailure::Superseded));
        }
        self.abort_retrieval(repo_root, retrieval_id, ctx);
    }

    #[cfg(feature = "local_fs")]
    fn abort_retrieval(
        &mut self,
        repo_root: &Path,
        retrieval_id: RetrievalID,
        ctx: &mut ModelContext<Self>,
    ) {
        let manager = CodebaseIndexManager::handle(ctx);
        manager.update(ctx, |manager, ctx| {
            if let Err(error) = manager.abort_retrieval_request(repo_root, retrieval_id, ctx) {
                // The index may have been dropped between starting and aborting.
                // Nothing is left running that we could still cancel, so there is
                // nothing for the user or for us to do about it.
                log::debug!("Could not abort a superseded codebase retrieval: {error:#}");
            }
        });
    }

    #[cfg(not(feature = "local_fs"))]
    fn abort_retrieval(
        &mut self,
        _repo_root: &Path,
        _retrieval_id: RetrievalID,
        _ctx: &mut ModelContext<Self>,
    ) {
    }

    /// Maps a completion event back to the request that started it.
    ///
    /// Completion events carry only the `RetrievalID`, so this is the only place the
    /// originating request can be recovered — the same reason the pin keeps
    /// `pending_request_details_for_retrieval_id`.
    #[cfg(feature = "local_fs")]
    fn handle_index_manager_event(
        &mut self,
        event: &CodebaseIndexManagerEvent,
        _ctx: &mut ModelContext<Self>,
    ) {
        match event {
            CodebaseIndexManagerEvent::RetrievalRequestCompleted {
                retrieval_id,
                ranked_paths,
                ..
            } => self.resolve(retrieval_id, Ok(ranked_paths.clone())),
            CodebaseIndexManagerEvent::RetrievalRequestFailed {
                retrieval_id,
                error_message,
            } => self.resolve(
                retrieval_id,
                Err(RetrievalFailure::Failed(error_message.clone())),
            ),
            _ => (),
        }
    }

    #[cfg_attr(not(feature = "local_fs"), allow(dead_code))]
    fn resolve(&mut self, retrieval_id: &RetrievalID, result: RetrievalResult) {
        // Retrievals started by anything other than this controller land here too;
        // they simply have no pending entry.
        let Some(pending) = self.pending.remove(retrieval_id) else {
            return;
        };
        self.in_flight_by_repo.remove(&pending.repo_root);
        // The waiter may have gone away (a cancelled turn). Its result is discarded,
        // which is the correct outcome and not worth logging.
        let _ = pending.responder.send(result);
    }

}

impl Entity for CodebaseRetrievalController {
    type Event = ();
}

impl SingletonEntity for CodebaseRetrievalController {}

/// A sendable, request-scoped ticket for querying one repository's embedding index.
///
/// Carried on `RequestParams` so the `chat_stream` tool interceptor — which is `async`
/// and has no `AppContext` — can start a retrieval anyway. Cheap to clone; holds no
/// strong reference to the controller.
#[derive(Clone)]
pub struct CodebaseRetrievalHandle {
    spawner: ModelSpawner<CodebaseRetrievalController>,
    repo_root: Arc<PathBuf>,
}

impl std::fmt::Debug for CodebaseRetrievalHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `ModelSpawner` is not `Debug`, and `RequestParams` is.
        f.debug_struct("CodebaseRetrievalHandle")
            .field("repo_root", &self.repo_root)
            .finish_non_exhaustive()
    }
}

impl CodebaseRetrievalHandle {
    /// The repository the index will be queried for. Retrieved paths are absolute and
    /// live under this root.
    pub fn repo_root(&self) -> &Path {
        self.repo_root.as_path()
    }

    /// Asks the embedding index which files are relevant to `query`.
    ///
    /// Hops to the main thread, starts the retrieval, and awaits the answer. Awaiting
    /// happens off the main thread, so the UI is not blocked; the turn is already
    /// suspended on a tool call.
    pub async fn retrieve(&self, query: &str) -> RetrievalResult {
        let repo_root = self.repo_root.as_ref().clone();
        let query = query.to_owned();

        let started = self
            .spawner
            .spawn(move |controller, ctx| controller.start_retrieval(repo_root, query, ctx))
            .await;

        match started {
            // The controller was dropped before it could start anything.
            Err(_) => Err(RetrievalFailure::Unavailable),
            Ok(Err(failure)) => Err(failure),
            // A dropped sender means the controller went away mid-flight.
            Ok(Ok(receiver)) => receiver.await.unwrap_or(Err(RetrievalFailure::Unavailable)),
        }
    }
}

/// A handle for querying the embedding index covering `directory`, or `None` when
/// there is nothing to query.
///
/// `None` — the common case, and the default configuration — costs nothing and must
/// stay that way: it is checked on every agent request. Returns `None` when the
/// codebase-context setting is off (so an index would not be maintained anyway), or
/// when no index covers `directory`, which includes remote sessions, since a remote
/// repository is indexed on its host and not in this process's store.
pub fn handle_for_directory(app: &AppContext, directory: &Path) -> Option<CodebaseRetrievalHandle> {
    if !crate::workspaces::user_workspaces::UserWorkspaces::as_ref(app)
        .is_codebase_context_enabled(app)
    {
        return None;
    }

    let repo_root = indexed_root_for(app, directory)?;
    Some(CodebaseRetrievalHandle {
        spawner: CodebaseRetrievalController::as_ref(app).spawner.clone(),
        repo_root: Arc::new(repo_root),
    })
}

/// The root of the indexed repository containing `directory`, if one is indexed.
#[cfg(feature = "local_fs")]
fn indexed_root_for(app: &AppContext, directory: &Path) -> Option<PathBuf> {
    CodebaseIndexManager::as_ref(app).root_path_for_codebase(directory)
}

#[cfg(not(feature = "local_fs"))]
fn indexed_root_for(_app: &AppContext, _directory: &Path) -> Option<PathBuf> {
    None
}

#[cfg(test)]
#[path = "codebase_retrieval_tests.rs"]
mod codebase_retrieval_tests;
