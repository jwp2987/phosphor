//! Remote codebase indexing, app side (Wave H parity P9b): the model that decides WHEN a
//! repository on an ssh host is (re)indexed, and the embedder that turns its text into vectors.
//!
//! The host (the `phosphor-remote` helper, or the app's own no-helper shell path) scans, chunks,
//! hashes and stores; `remote_server::backend::index_sync` is the algorithm. What lives here is
//! what only the app can do:
//!
//! * [`GateEmbedder`] embeds through `crates/network_gate` (`HttpEmbeddingProvider` is built on
//!   `http_client::Client::for_purpose(AI_ENDPOINT)`) with the user's own BYOP `/embeddings`
//!   provider. The key is read from the keychain on this machine and goes to the provider
//!   and nowhere else: not to the helper, not to the shell, not into a request to the host.
//! * [`RemoteIndexDriver`] triggers syncs on EVENTS only: navigating into a git
//!   repository, the app's own write to the host, a prompt that follows a HEAD-changing git
//!   command, the user asking, and the setting being turned on. One sync per repository runs at
//!   a time; events that arrive meanwhile queue exactly one more. The two PROMPT events
//!   (navigating, a HEAD-changing command) do not start the scan at once (#2156): a scan is many
//!   exec channels on a host without the helper, and a prompt may cost at most one. They start
//!   it once the repository has been quiet for [`PROMPT_SYNC_DELAY`], a later prompt event
//!   replacing the pending start, so any number of prompts in a row cost one scan, after them.
//!
//! The feature is off unless the user turned it on (`should_use_codebase_indexing`: the
//! `FullSourceCodeEmbedding` flag AND the codebase-context setting): with it off nothing here
//! scans, embeds or reads a key.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use ::ai::index::full_source_code_embedding::EmbeddingConfig;
use ::ai::index::full_source_code_embedding::local_store_client::EmbeddingProvider;
use futures::StreamExt as _;
use futures::future::BoxFuture;
use remote_server::backend::index_sync::{self, Embedder, SyncError, SyncOutcome, SyncProgress};
use remote_server::backend::refresh::RemoteRefreshEvent;
use remote_server::backend::{Family, RoutedBackend};
use remote_server::manager::{RemoteServerManager, RemoteServerManagerEvent};
use remote_server::shellcmd::predicates::command_may_change_head;
use warp_core::{HostId, SessionId};
use warpui::r#async::SpawnedFutureHandle;
use warpui::{Entity, ModelContext, SingletonEntity};

use crate::ai::agent_providers::embeddings::{HttpEmbeddingProvider, resolve_embedding_endpoints};
use crate::ai::codebase_auto_indexing::{
    CodebaseAutoIndexingSurface, should_auto_index_codebase, should_use_codebase_indexing,
};
use crate::settings::CodeSettings;

/// Embeds through the network gate with the user's own provider. Built on the main thread from
/// the settings as they are NOW (a snapshot: a sync uses one provider throughout), then used
/// from any thread.
pub struct GateEmbedder {
    provider: Arc<HttpEmbeddingProvider>,
    config: EmbeddingConfig,
    batch: usize,
}

impl GateEmbedder {
    /// `None` when the user has no usable embedding provider. `prefer` is the storage key the
    /// repository was last indexed with: kept while its provider is still configured, so adding
    /// a second provider does not silently re-embed the repository on the user's quota.
    pub fn from_app(app: &warpui::AppContext, prefer: Option<&str>) -> Option<Self> {
        let endpoints = resolve_embedding_endpoints(app);
        let config = choose_config(&endpoints, prefer)?;
        let provider = HttpEmbeddingProvider::new(
            http_client::Client::for_purpose(&http_client::gate::AI_ENDPOINT),
            None,
        );
        provider.set_endpoints(endpoints);
        let batch = crate::ai::AIRequestUsageModel::as_ref(app)
            .codebase_context_limits()
            .embedding_generation_batch_size
            .max(1);
        Some(Self {
            provider: Arc::new(provider),
            config,
            batch,
        })
    }
}

/// The model to embed with: `prefer` (the storage key the repository was last indexed with)
/// while some configured provider still serves it, else the first configured model. `None` when
/// no provider is configured.
fn choose_config(
    endpoints: &crate::ai::agent_providers::embeddings::EmbeddingEndpoints,
    prefer: Option<&str>,
) -> Option<EmbeddingConfig> {
    prefer
        .and_then(EmbeddingConfig::from_storage_key)
        .filter(|c| endpoints.get(*c).is_some())
        .or_else(|| endpoints.preferred_model())
}

impl Embedder for GateEmbedder {
    fn storage_key(&self) -> Option<String> {
        Some(self.config.storage_key().to_string())
    }

    fn max_batch(&self) -> usize {
        self.batch
    }

    fn embed(&self, texts: Vec<String>) -> BoxFuture<'_, Result<Vec<Vec<f32>>, String>> {
        Box::pin(async move {
            // One line per request, so a test (and a user) can see the key's only destination is
            // the gate: the count and the purpose, never the text, the URL or the key.
            log::info!(
                "hop=remote_index_embed texts={} via=network_gate purpose=ai_endpoint",
                texts.len()
            );
            self.provider
                .embed(self.config, texts)
                .await
                .map_err(|e| e.to_string())
        })
    }
}

/// Why a sync was started, for the log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncReason {
    Navigated,
    OwnWrite,
    HeadChanged,
    UserAction,
    SettingEnabled,
}

/// How long a repository must be quiet (no further navigation or HEAD-changing prompt) before a
/// sync that a prompt asked for starts. It keeps the scan's exec channels out of the prompt that
/// caused them (#2156: one prompt, at most one side exec on a host with no helper).
pub const PROMPT_SYNC_DELAY: Duration = Duration::from_secs(15);

/// `PHOSPHOR_REMOTE_INDEX_DEFER=0` (read once) starts prompt-driven syncs at once again.
fn defer_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("PHOSPHOR_REMOTE_INDEX_DEFER").map_or(true, |v| v.trim() != "0")
    })
}

/// The delay before a sync for `reason` starts: only the prompt-driven reasons wait.
pub fn start_delay(reason: SyncReason) -> Option<Duration> {
    if !defer_enabled() {
        return None;
    }
    match reason {
        SyncReason::Navigated | SyncReason::HeadChanged => Some(PROMPT_SYNC_DELAY),
        SyncReason::OwnWrite | SyncReason::UserAction | SyncReason::SettingEnabled => None,
    }
}

/// What the driver knows about one repository's index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteIndexStatus {
    Scanning,
    Embedding {
        completed: usize,
        total: usize,
    },
    Ready {
        root_hash: String,
        fragments: usize,
    },
    /// The user has no embedding provider configured.
    NoProvider,
    Failed(String),
}

#[derive(Default)]
struct RepoState {
    session_id: Option<SessionId>,
    status: Option<RemoteIndexStatus>,
    running: bool,
    /// An event arrived while a sync ran: run once more when it ends.
    dirty: bool,
    /// The storage key of the last completed sync, to keep it across provider-list edits.
    storage_key: Option<String>,
    /// A prompt-driven sync waiting for the repository to be quiet; replaced by a later one,
    /// dropped when a sync starts.
    pending: Option<SpawnedFutureHandle>,
}

type RepoKey = (HostId, String);

/// Messages from background syncs and from the router's refresh listener to the model.
enum Note {
    Progress(RepoKey, SyncProgress),
    Finished(RepoKey, Result<SyncOutcome, SyncError>, Option<String>),
    OwnWrite(HostId),
}

pub struct RemoteIndexDriver {
    notes: async_channel::Sender<Note>,
    repos: HashMap<RepoKey, RepoState>,
    /// The repository each session last navigated into.
    active: HashMap<SessionId, String>,
}

#[derive(Clone, Debug)]
pub enum RemoteIndexDriverEvent {
    StatusChanged,
}

impl Entity for RemoteIndexDriver {
    type Event = RemoteIndexDriverEvent;
}

impl SingletonEntity for RemoteIndexDriver {}

impl RemoteIndexDriver {
    pub fn new(ctx: &mut ModelContext<Self>) -> Self {
        let (notes, inbox) = async_channel::unbounded();
        let _ = ctx.spawn_stream_local(inbox, Self::on_note, |_, _| {});

        let manager = RemoteServerManager::handle(ctx);
        ctx.subscribe_to_model(&manager, |me, event, ctx| {
            me.on_manager_event(event, ctx);
        });
        // The app's own writes to a host: the router tells us from whatever thread made them.
        let tx = notes.clone();
        RemoteServerManager::as_ref(ctx)
            .router()
            .subscribe_refresh(Arc::new(move |host, event| {
                if matches!(event, RemoteRefreshEvent::AfterOwnWrite) {
                    let _ = tx.try_send(Note::OwnWrite(host.clone()));
                }
            }));
        // Turning the setting on indexes the repositories sessions are already in.
        ctx.subscribe_to_model(&CodeSettings::handle(ctx), |me, _event, ctx| {
            me.sync_active_repos(SyncReason::SettingEnabled, ctx);
        });
        Self {
            notes,
            repos: HashMap::new(),
            active: HashMap::new(),
        }
    }

    /// The repository `session_id` last navigated into.
    pub fn active_repo(&self, session_id: SessionId) -> Option<&str> {
        self.active.get(&session_id).map(String::as_str)
    }

    /// The driver's view of `repo` for the host `session_id` reaches.
    pub fn status(
        &self,
        session_id: SessionId,
        repo: &str,
        app: &warpui::AppContext,
    ) -> Option<RemoteIndexStatus> {
        let host = RemoteServerManager::as_ref(app).host_for_session_or_shell_mode(session_id);
        self.repos
            .get(&(host, repo.to_string()))
            .and_then(|s| s.status.clone())
    }

    /// The storage key the repository was last indexed with, if any.
    pub fn last_storage_key(
        &self,
        session_id: SessionId,
        repo: &str,
        app: &warpui::AppContext,
    ) -> Option<String> {
        let host = RemoteServerManager::as_ref(app).host_for_session_or_shell_mode(session_id);
        self.repos
            .get(&(host, repo.to_string()))
            .and_then(|s| s.storage_key.clone())
    }

    /// The user asked to refresh the active repository's index.
    pub fn user_resync(&mut self, session_id: SessionId, ctx: &mut ModelContext<Self>) {
        if let Some(repo) = self.active.get(&session_id).cloned() {
            self.request_sync(session_id, repo, SyncReason::UserAction, ctx);
        }
    }

    /// The prompt came back after `command`: re-index when it can have moved HEAD (a pull, a
    /// checkout, a merge, a rebase, a reset). Any other command changes nothing the index would
    /// miss that a later own-write or navigation event does not cover.
    pub fn on_prompt(
        &mut self,
        session_id: SessionId,
        command: &str,
        ctx: &mut ModelContext<Self>,
    ) {
        if !command_may_change_head(command) {
            return;
        }
        if let Some(repo) = self.active.get(&session_id).cloned() {
            self.request_sync_soon(session_id, repo, SyncReason::HeadChanged, ctx);
        }
    }

    /// [`Self::request_sync`] after the repository has been quiet for the reason's delay (see
    /// [`start_delay`]); a newer request for the same repository replaces the pending one.
    fn request_sync_soon(
        &mut self,
        session_id: SessionId,
        repo: String,
        reason: SyncReason,
        ctx: &mut ModelContext<Self>,
    ) {
        let Some(delay) = start_delay(reason) else {
            self.request_sync(session_id, repo, reason, ctx);
            return;
        };
        let host = RemoteServerManager::as_ref(ctx).host_for_session_or_shell_mode(session_id);
        let key: RepoKey = (host, repo.clone());
        let state = self.repos.entry(key.clone()).or_default();
        if let Some(previous) = state.pending.take() {
            previous.abort();
        }
        log::info!(
            "hop=remote_index_sync deferred reason={reason:?} delay_ms={}",
            delay.as_millis()
        );
        state.pending = Some(ctx.spawn(
            async move {
                warpui::r#async::Timer::after(delay).await;
            },
            move |me, _, ctx| {
                if let Some(state) = me.repos.get_mut(&key) {
                    state.pending = None;
                }
                me.request_sync(session_id, repo, reason, ctx);
            },
        ));
    }

    fn on_manager_event(&mut self, event: &RemoteServerManagerEvent, ctx: &mut ModelContext<Self>) {
        match event {
            RemoteServerManagerEvent::NavigatedToDirectory {
                session_id,
                indexed_path,
                is_git,
                ..
            } => {
                self.active.insert(*session_id, indexed_path.clone());
                if *is_git && should_auto_index_codebase(CodebaseAutoIndexingSurface::Remote, ctx) {
                    self.request_sync_soon(
                        *session_id,
                        indexed_path.clone(),
                        SyncReason::Navigated,
                        ctx,
                    );
                }
            }
            // Only when the session is gone for good: a lost helper connection leaves the
            // shell path working, and the repository still active.
            RemoteServerManagerEvent::SessionDeregistered { session_id } => {
                self.active.remove(session_id);
            }
            _ => {}
        }
    }

    fn sync_active_repos(&mut self, reason: SyncReason, ctx: &mut ModelContext<Self>) {
        if !should_auto_index_codebase(CodebaseAutoIndexingSurface::Remote, ctx) {
            return;
        }
        let targets: Vec<(SessionId, String)> =
            self.active.iter().map(|(s, r)| (*s, r.clone())).collect();
        for (session_id, repo) in targets {
            self.request_sync(session_id, repo, reason, ctx);
        }
    }

    fn on_note(&mut self, note: Note, ctx: &mut ModelContext<Self>) {
        match note {
            Note::Progress(key, progress) => {
                if let Some(state) = self.repos.get_mut(&key) {
                    state.status = Some(match progress {
                        SyncProgress::Scanning => RemoteIndexStatus::Scanning,
                        SyncProgress::Embedding { completed, total } => {
                            RemoteIndexStatus::Embedding { completed, total }
                        }
                    });
                    ctx.emit(RemoteIndexDriverEvent::StatusChanged);
                }
            }
            Note::Finished(key, result, storage_key) => {
                let (session_id, again) = {
                    let Some(state) = self.repos.get_mut(&key) else {
                        return;
                    };
                    state.running = false;
                    state.status = Some(match &result {
                        Ok(outcome) => RemoteIndexStatus::Ready {
                            root_hash: outcome.root_hash.clone(),
                            fragments: outcome.fragments,
                        },
                        Err(SyncError::NoProvider) => RemoteIndexStatus::NoProvider,
                        Err(e) => RemoteIndexStatus::Failed(e.to_string()),
                    });
                    if result.is_ok() {
                        state.storage_key = storage_key;
                    }
                    (state.session_id, std::mem::take(&mut state.dirty))
                };
                match &result {
                    Ok(o) => log::info!(
                        "hop=remote_index_sync done files={} fragments={} embedded={} already={} stale={}",
                        o.files,
                        o.fragments,
                        o.embedded,
                        o.already,
                        o.left_stale
                    ),
                    Err(SyncError::NoProvider) => {
                        log::info!("hop=remote_index_sync skipped reason=no_provider")
                    }
                    Err(e) => log::warn!("hop=remote_index_sync failed: {e}"),
                }
                ctx.emit(RemoteIndexDriverEvent::StatusChanged);
                if again && let Some(session_id) = session_id {
                    self.request_sync(session_id, key.1, SyncReason::OwnWrite, ctx);
                }
            }
            Note::OwnWrite(host) => {
                let targets: Vec<(SessionId, String)> = self
                    .repos
                    .iter()
                    .filter(|((h, _), s)| *h == host && s.session_id.is_some())
                    .filter_map(|((_, repo), s)| Some((s.session_id?, repo.clone())))
                    .collect();
                for (session_id, repo) in targets {
                    self.request_sync(session_id, repo, SyncReason::OwnWrite, ctx);
                }
            }
        }
    }

    /// Starts a sync of `repo` as reached from `session_id`, or queues one behind the running
    /// sync. A no-op when the feature is off or the host has no way to serve it.
    pub fn request_sync(
        &mut self,
        session_id: SessionId,
        repo: String,
        reason: SyncReason,
        ctx: &mut ModelContext<Self>,
    ) {
        if !should_use_codebase_indexing(CodebaseAutoIndexingSurface::Remote, ctx) {
            return;
        }
        let (host, backend): (HostId, RoutedBackend) = {
            let manager = RemoteServerManager::as_ref(ctx);
            (
                manager.host_for_session_or_shell_mode(session_id),
                manager.backend_for_session(session_id),
            )
        };
        if !backend.is_available(Family::CodebaseChunks) {
            log::debug!("remote index: no way to reach the host for {session_id:?}");
            return;
        }
        let key: RepoKey = (host, repo.clone());
        let state = self.repos.entry(key.clone()).or_default();
        state.session_id = Some(session_id);
        // A sync starting (or queued) now covers what a pending prompt-driven one would.
        if let Some(pending) = state.pending.take() {
            pending.abort();
        }
        if state.running {
            state.dirty = true;
            return;
        }
        let prefer = state.storage_key.clone();
        let Some(embedder) = GateEmbedder::from_app(ctx, prefer.as_deref()) else {
            state.status = Some(RemoteIndexStatus::NoProvider);
            ctx.emit(RemoteIndexDriverEvent::StatusChanged);
            log::info!("hop=remote_index_sync skipped reason=no_provider");
            return;
        };
        state.running = true;
        state.status = Some(RemoteIndexStatus::Scanning);
        ctx.emit(RemoteIndexDriverEvent::StatusChanged);
        log::info!("hop=remote_index_sync start reason={reason:?}");

        let notes = self.notes.clone();
        let storage_key = embedder.storage_key();
        ctx.background_executor()
            .spawn(async move {
                let progress_notes = notes.clone();
                let progress_key = key.clone();
                let result = index_sync::sync_repo(&backend, &embedder, &repo, move |p| {
                    let _ = progress_notes.try_send(Note::Progress(progress_key.clone(), p));
                })
                .await;
                let _ = notes.send(Note::Finished(key, result, storage_key)).await;
            })
            .detach();
    }
}

// Compile-time check that the pieces the sync future needs are `Send`.
const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<GateEmbedder>();
    assert_send::<RoutedBackend>();
};

#[cfg(test)]
#[path = "remote_index_driver_tests.rs"]
mod tests;
