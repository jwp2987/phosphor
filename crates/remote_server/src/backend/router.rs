//! The ONE place that decides helper or shell for a feature family.
//!
//! The decision is a pure function, [`decide`]; [`FeatureRouter`] adds the two pieces of state it
//! needs (which families a connection has answered `UNSUPPORTED` or died on, and the per-family
//! "always shell" override) and the registry of out-of-band executors per session.
//! [`RoutedBackend`] is what call sites hold: each method asks the router, calls the chosen
//! backend, and on an `UNSUPPORTED` (or a lost connection for a read) serves the same call on the
//! shell. Call sites never retry. No timers.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use warp_core::SessionId;

use super::BufferSaveOutcome;
use super::executor::OutOfBandExecutor;
use super::helper::client_id;
use super::refresh::RemoteRefreshEvent;
use super::shell::{
    AgentContextCache, GitBatchStore, PrLookup, ShellBackend, ShellBuffers, ShellIndex, ShellPush,
    TreeCache,
};
use super::{Family, Requested};
use crate::HostId;
use crate::capability::ServerCapabilities;
use crate::client::RemoteServerClient;
use warp_util::standardized_path::StandardizedPath;

/// Which backend serves a family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendKind {
    Helper,
    Shell,
}

/// What the router needs to know about a host's helper connection.
#[derive(Clone, Debug)]
pub struct HelperProbe {
    /// Identity of the connection (a reconnect is a different one).
    pub client_id: usize,
    pub connected: bool,
    pub capabilities: ServerCapabilities,
}

impl HelperProbe {
    pub fn of(client: &Arc<RemoteServerClient>) -> Self {
        Self {
            client_id: client_id(client),
            connected: !client.is_disconnected(),
            capabilities: client.capabilities(),
        }
    }
}

/// The pure decision. `helper` is `None` when no helper is connected for the host.
///
/// Helper when: connected, not overridden to shell, has not failed this family on this
/// connection, and its capabilities support the family (an empty list = legacy full daemon =
/// everything). Shell otherwise.
pub fn decide(
    helper: Option<&HelperProbe>,
    family: Family,
    always_shell: bool,
    fell_back: bool,
) -> BackendKind {
    match helper {
        Some(h)
            if h.connected
                && !always_shell
                && !fell_back
                && h.capabilities.supports(family.capability()) =>
        {
            BackendKind::Helper
        }
        _ => BackendKind::Shell,
    }
}

/// The `input_order`-style line the lab asserts routing from:
/// `hop=backend_route family=<capability> backend=<helper|shell|unavailable> reason=<...>`.
pub fn route_log_line(family: Family, backend: &str, reason: &str) -> String {
    format!(
        "hop=backend_route family={} backend={backend} reason={reason}",
        family.capability()
    )
}

/// The route line for a call the app serves through the session's command executor rather than a
/// `RoutedBackend` method (the agent's grep and glob run as session commands): `helper_serves` is
/// whether that executor is the remote helper's. Same formatter and reasons as the router's own
/// lines (`caps`: the arm the host's capabilities leave).
pub fn executor_route_line(family: Family, helper_serves: bool) -> String {
    route_log_line(
        family,
        if helper_serves { "helper" } else { "shell" },
        "caps",
    )
}

/// Per-family "always shell" override, for tests and the lab (the helper is never asked).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AlwaysShell {
    all: bool,
    families: HashSet<Family>,
}

impl AlwaysShell {
    pub const ENV_VAR: &'static str = "PHOSPHOR_REMOTE_ALWAYS_SHELL";

    pub fn none() -> Self {
        Self::default()
    }

    pub fn all() -> Self {
        Self {
            all: true,
            families: HashSet::new(),
        }
    }

    pub fn only(families: impl IntoIterator<Item = Family>) -> Self {
        Self {
            all: false,
            families: families.into_iter().collect(),
        }
    }

    /// Parses `all` / `1` or a comma-separated list of capability names (`file_context,git_write`).
    /// Unknown names are ignored.
    pub fn parse(value: Option<&str>) -> Self {
        let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) else {
            return Self::none();
        };
        if matches!(value, "all" | "1" | "true") {
            return Self::all();
        }
        Self::only(
            value
                .split(',')
                .filter_map(|name| Family::from_capability(name.trim())),
        )
    }

    pub fn from_env() -> Self {
        Self::parse(std::env::var(Self::ENV_VAR).ok().as_deref())
    }

    pub fn contains(&self, family: Family) -> bool {
        self.all || self.families.contains(&family)
    }
}

#[derive(Default)]
struct Fallbacks {
    client_id: usize,
    families: HashSet<Family>,
}

#[derive(Default)]
struct State {
    always_shell: AlwaysShell,
    executors: HashMap<SessionId, Arc<dyn OutOfBandExecutor>>,
    fallbacks: HashMap<HostId, Fallbacks>,
    /// Per-host shell-mode file trees, shared by every backend built for the host.
    tree_caches: HashMap<HostId, Arc<TreeCache>>,
    /// Where the shell arm keeps its codebase indexes; `None` leaves indexing unconfigured.
    index_root: Option<std::path::PathBuf>,
    /// A stable name for the machine behind a session (`user@host`), which keys its shell-arm
    /// index across reconnects. A session with none is keyed by its host id.
    identities: HashMap<SessionId, String>,
    indexes: HashMap<String, Arc<ShellIndex>>,
    /// Per-host shell-mode agent context (last snapshot and revision).
    agent_caches: HashMap<HostId, Arc<AgentContextCache>>,
    /// Per-host shell-mode buffer states (family c), shared likewise.
    buffer_states: HashMap<HostId, Arc<ShellBuffers>>,
    /// Per-host batched git answers (#2156), shared likewise.
    git_batches: HashMap<HostId, Arc<GitBatchStore>>,
    refresh_listeners: Vec<RefreshListener>,
}

/// Called with the host and the event whenever the app changes a host itself (see
/// [`FeatureRouter::subscribe_refresh`]).
pub type RefreshListener = Arc<dyn Fn(&HostId, &RemoteRefreshEvent) + Send + Sync>;

/// Shared handle; clones see the same state.
#[derive(Clone, Default)]
pub struct FeatureRouter {
    state: Arc<Mutex<State>>,
}

impl std::fmt::Debug for FeatureRouter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FeatureRouter").finish_non_exhaustive()
    }
}

impl FeatureRouter {
    pub fn new() -> Self {
        Self::default()
    }

    /// A router with the override read from `PHOSPHOR_REMOTE_ALWAYS_SHELL`.
    pub fn from_env() -> Self {
        let router = Self::new();
        router.set_always_shell(AlwaysShell::from_env());
        router
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn set_always_shell(&self, always_shell: AlwaysShell) {
        self.lock().always_shell = always_shell;
    }

    /// Registers the out-of-band channel of `session`. The app calls this when a session's
    /// executor is a ControlMaster (or other non-pty) one; a session without a channel is simply
    /// never registered and its shell backend is `Unavailable`.
    pub fn register_out_of_band(&self, session: SessionId, executor: Arc<dyn OutOfBandExecutor>) {
        self.lock().executors.insert(session, executor);
    }

    pub fn unregister_out_of_band(&self, session: SessionId) {
        let mut state = self.lock();
        state.executors.remove(&session);
        state.identities.remove(&session);
    }

    /// Where the shell arm keeps the codebase indexes it builds (the app's data directory).
    pub fn set_index_root(&self, root: std::path::PathBuf) {
        self.lock().index_root = Some(root);
    }

    /// Names the machine behind `session` stably (`user@host`), so its shell-arm index outlives
    /// the session.
    pub fn set_session_identity(&self, session: SessionId, identity: String) {
        self.lock().identities.insert(session, identity);
    }

    /// The shell-arm index state for `host` reached through `sessions`, if storage is
    /// configured.
    fn index_for(&self, host: &HostId, sessions: &[SessionId]) -> Option<Arc<ShellIndex>> {
        let mut state = self.lock();
        let root = state.index_root.clone()?;
        let identity = sessions
            .iter()
            .find_map(|s| state.identities.get(s).cloned())
            .unwrap_or_else(|| host.as_str().to_string());
        Some(Arc::clone(
            state
                .indexes
                .entry(identity.clone())
                .or_insert_with(|| Arc::new(ShellIndex::new(root, identity))),
        ))
    }

    /// Registers a listener for [`RemoteRefreshEvent`]s the backend itself raises: today
    /// `AfterOwnWrite` after every successful mutating call (a save, a delete, a git write), on
    /// either arm. A shell-backed view re-reads on it; nothing here is a timer.
    pub fn subscribe_refresh(&self, listener: RefreshListener) {
        self.lock().refresh_listeners.push(listener);
    }

    /// Tells every listener that `event` happened on `host`.
    pub fn emit_refresh(&self, host: &HostId, event: &RemoteRefreshEvent) {
        // Outside the lock: a listener may call back into the router.
        let listeners = self.lock().refresh_listeners.clone();
        for listener in listeners {
            listener(host, event);
        }
    }

    /// The first registered channel among `sessions`.
    pub fn executor_for(
        &self,
        sessions: impl IntoIterator<Item = SessionId>,
    ) -> Option<Arc<dyn OutOfBandExecutor>> {
        let state = self.lock();
        sessions
            .into_iter()
            .find_map(|s| state.executors.get(&s).cloned())
    }

    /// Which backend serves `family` for `host` right now.
    pub fn choose(
        &self,
        host: &HostId,
        family: Family,
        helper: Option<&HelperProbe>,
    ) -> BackendKind {
        let mut state = self.lock();
        if let Some(probe) = helper {
            // A different connection (reconnect) starts with a clean slate.
            if state
                .fallbacks
                .get(host)
                .is_some_and(|f| f.client_id != probe.client_id)
            {
                state.fallbacks.remove(host);
            }
        }
        let fell_back = helper.is_some_and(|probe| {
            state
                .fallbacks
                .get(host)
                .is_some_and(|f| f.client_id == probe.client_id && f.families.contains(&family))
        });
        decide(
            helper,
            family,
            state.always_shell.contains(family),
            fell_back,
        )
    }

    /// Why `choose` picked what it did, for the `backend_route` log line.
    fn route_reason(
        &self,
        host: &HostId,
        family: Family,
        helper: Option<&HelperProbe>,
        shell_available: bool,
    ) -> (&'static str, &'static str) {
        let kind = self.choose(host, family, helper);
        if kind == BackendKind::Helper {
            return ("helper", "caps");
        }
        if !shell_available {
            return ("unavailable", "no_channel");
        }
        let state = self.lock();
        let reason = if state.always_shell.contains(family) {
            "override"
        } else if helper.is_some_and(|p| {
            state
                .fallbacks
                .get(host)
                .is_some_and(|f| f.client_id == p.client_id && f.families.contains(&family))
        }) {
            if helper.is_some_and(|p| p.connected) {
                "unsupported_fallback"
            } else {
                "connection_lost"
            }
        } else if helper.is_some_and(|p| !p.connected) {
            "connection_lost"
        } else {
            "caps"
        };
        ("shell", reason)
    }

    /// Records that `family` must be served by the shell on this connection from now on.
    pub fn note_fallback(&self, host: &HostId, client_id: usize, family: Family) {
        let mut state = self.lock();
        let entry = state.fallbacks.entry(host.clone()).or_default();
        if entry.client_id != client_id {
            entry.client_id = client_id;
            entry.families.clear();
        }
        entry.families.insert(family);
    }

    /// The backend for `family` on `host`. `helper` is the host's connected client, if any;
    /// `sessions` are the host's sessions (for the shell's out-of-band channel).
    pub fn backend(
        &self,
        host: &HostId,
        helper: Option<Arc<RemoteServerClient>>,
        sessions: impl IntoIterator<Item = SessionId>,
    ) -> RoutedBackend {
        let sessions: Vec<SessionId> = sessions.into_iter().collect();
        let mut shell = ShellBackend::new(self.executor_for(sessions.iter().copied()))
            .with_tree_cache(self.tree_cache_for(host))
            .with_agent_cache(self.agent_cache_for(host))
            .with_buffers(self.buffers_for(host))
            .with_git_batch(self.git_batch_for(host));
        if let Some(index) = self.index_for(host, &sessions) {
            shell = shell.with_index(index);
        }
        RoutedBackend {
            router: self.clone(),
            host: host.clone(),
            helper,
            shell,
        }
    }

    /// The shell-mode agent context of `host` (created on first use).
    pub fn agent_cache_for(&self, host: &HostId) -> Arc<AgentContextCache> {
        Arc::clone(self.lock().agent_caches.entry(host.clone()).or_default())
    }

    /// Forgets `host`'s shell-mode agent context (the host disconnected). The revision counter
    /// survives so a later context for the same host id never goes backwards.
    pub fn forget_agent_context(&self, host: &HostId) {
        if let Some(cache) = self.lock().agent_caches.get(host) {
            cache.clear();
        }
    }

    /// The shell-mode file-tree state of `host` (created on first use).
    pub fn tree_cache_for(&self, host: &HostId) -> Arc<TreeCache> {
        Arc::clone(self.lock().tree_caches.entry(host.clone()).or_default())
    }

    /// The shell-mode batched git answers of `host` (created on first use).
    pub fn git_batch_for(&self, host: &HostId) -> Arc<GitBatchStore> {
        Arc::clone(self.lock().git_batches.entry(host.clone()).or_default())
    }

    /// The shell-mode buffer states of `host` (created on first use).
    pub fn buffers_for(&self, host: &HostId) -> Arc<ShellBuffers> {
        Arc::clone(self.lock().buffer_states.entry(host.clone()).or_default())
    }

    /// Forgets `host`'s shell-mode file trees (the host disconnected).
    pub fn forget_trees(&self, host: &HostId) {
        if let Some(cache) = self.lock().tree_caches.remove(host) {
            cache.clear();
        }
    }
}

/// What a call site holds for one host: implements
/// [`RemoteFeatures`](super::RemoteFeatures) and routes each call (see the module docs).
#[derive(Clone)]
pub struct RoutedBackend {
    router: FeatureRouter,
    host: HostId,
    helper: Option<Arc<RemoteServerClient>>,
    shell: ShellBackend,
}

impl std::fmt::Debug for RoutedBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RoutedBackend")
            .field("host", &self.host)
            .field("has_helper", &self.helper.is_some())
            .field("shell", &self.shell)
            .finish()
    }
}

impl RoutedBackend {
    pub(super) fn router(&self) -> &FeatureRouter {
        &self.router
    }

    pub(super) fn host(&self) -> &HostId {
        &self.host
    }

    pub(super) fn helper_client(&self) -> Option<&Arc<RemoteServerClient>> {
        self.helper.as_ref()
    }

    pub(super) fn shell(&self) -> &ShellBackend {
        &self.shell
    }

    /// Logs the routing decision of one call: family, backend and why, no paths or content.
    pub(super) fn log_route(&self, family: Family) {
        let probe = self.helper.as_ref().map(HelperProbe::of);
        let (backend, reason) = self.router.route_reason(
            &self.host,
            family,
            probe.as_ref(),
            self.shell.is_available(),
        );
        log::info!("{}", route_log_line(family, backend, reason));
    }

    /// Which backend a call for `family` would use now. For UI (the "shell mode" chip) and tests.
    pub fn kind(&self, family: Family) -> BackendKind {
        let probe = self.helper.as_ref().map(HelperProbe::of);
        self.router.choose(&self.host, family, probe.as_ref())
    }

    /// Reads the host's agent context (skills and global rules) when the shell serves
    /// `AgentContext` (a helper that has the capability pushes it by itself, so this is then a
    /// no-op) and returns the pushes produced, ready to emit: the snapshot when it changed.
    /// Drive it at session start and on an explicit refresh; it never runs on a timer.
    pub async fn refresh_agent_context(&self) -> Result<Vec<ShellPush>, super::BackendError> {
        if self.kind(Family::AgentContext) == BackendKind::Shell {
            self.shell.refresh_agent_context().await?;
        }
        Ok(self.shell.take_pushes())
    }

    /// Re-reads the file tree of the indexed root `repo_path` when the shell serves
    /// `RepoMetadata` (the helper pushes its own updates, so this is then a no-op) and returns
    /// the pushes this backend produced, ready to emit. Drive it from `RefreshCoordinator`
    /// events; it never runs on a timer.
    pub async fn refresh_repo_tree(
        &self,
        repo_path: &str,
    ) -> Result<Vec<ShellPush>, super::BackendError> {
        if self.kind(Family::RepoMetadata) == BackendKind::Shell {
            self.shell.refresh_repo_tree(repo_path).await?;
        }
        Ok(self.shell.take_pushes())
    }

    /// The pushes shell-mode calls on this backend produced so far (file-tree snapshots after a
    /// `navigate_to_directory`). Empty when the helper served the call: it pushes by itself.
    pub fn take_shell_pushes(&self) -> Vec<ShellPush> {
        self.shell.take_pushes()
    }

    /// [`RemoteFeatures::delete_file`](super::RemoteFeatures::delete_file) that, when `expect` is
    /// given, deletes only if the file is still in that state (the same contract as
    /// [`Self::write_file_expecting`]): a helper that lists `file_delete_expect` checks it
    /// itself, any other helper or a legacy daemon is bypassed for the guarded shell script, and
    /// with no shell channel the delete is REFUSED, never sent unguarded. Emits `AfterOwnWrite`
    /// on success.
    pub async fn delete_file_expecting(
        &self,
        path: String,
        expect: Option<crate::shellcmd::write::Expect>,
    ) -> Result<(), super::BackendError> {
        use super::RemoteFeatures as _;
        let Some(expect) = expect else {
            return self.delete_file(path).await;
        };
        self.log_route(Family::FileWrite);
        let helper_guard = self.helper_for(Family::FileWrite).filter(|client| {
            client
                .capabilities()
                .supports_listed(crate::capability::FILE_DELETE_EXPECT)
        });
        let result = match (helper_guard, expect.to_wire()) {
            (Some(client), Some(wire)) => client
                .delete_file_expecting(path, Some(wire))
                .await
                .map_err(super::BackendError::from),
            _ if self.shell.is_available() => {
                self.shell.delete_file_expecting(path, Some(expect)).await
            }
            _ => Err(super::BackendError::Unavailable(format!(
                "cannot delete {path} safely: the host cannot check that it is unchanged \
                 (no helper with delete checks and no shell channel), so nothing was deleted"
            ))),
        };
        if result.is_ok() {
            self.router
                .emit_refresh(&self.host, &RemoteRefreshEvent::AfterOwnWrite);
        }
        result
    }

    /// [`RemoteFeatures::write_file`](super::RemoteFeatures::write_file) that, when `expect` is
    /// given, writes only if the file is still in that state; on a mismatch nothing is written
    /// and the error says the file changed (#1837). A helper that lists `file_write_expect`
    /// checks it itself (`WriteFile.expect`, `Sha256` and `Absent` only); any other helper, or a
    /// legacy daemon, ignores the field, so the write goes through the shell arm instead (the
    /// guarded script), and with no shell arm it is REFUSED, never sent unguarded. Emits
    /// `AfterOwnWrite` like any write.
    pub async fn write_file_expecting(
        &self,
        path: String,
        content: String,
        expect: Option<crate::shellcmd::write::Expect>,
    ) -> Result<(), super::BackendError> {
        use super::RemoteFeatures as _;
        let Some(expect) = expect else {
            return self.write_file(path, content).await;
        };
        self.log_route(Family::FileWrite);
        let helper_guard = self.helper_for(Family::FileWrite).filter(|client| {
            client
                .capabilities()
                .supports_listed(crate::capability::FILE_WRITE_EXPECT)
        });
        let result = match (helper_guard, expect.to_wire()) {
            (Some(client), Some(wire)) => client
                .write_file_expecting(path, content, Some(wire))
                .await
                .map_err(super::BackendError::from),
            _ if self.shell.is_available() => {
                self.shell
                    .write_file_expecting(path, content, Some(expect))
                    .await
            }
            _ => Err(super::BackendError::Unavailable(format!(
                "cannot write {path} safely: the host cannot check that it is unchanged \
                 (no helper with write checks and no shell channel), so nothing was written"
            ))),
        };
        if result.is_ok() {
            self.router
                .emit_refresh(&self.host, &RemoteRefreshEvent::AfterOwnWrite);
        }
        result
    }

    /// Tells the host a `BufferEdit`. The helper keeps a copy of the buffer, so the edit goes
    /// there; with the shell serving buffers there is no server copy, the editor's text is the
    /// only one and this does nothing. An `Err` means the edit could not be delivered (the
    /// helper connection is gone): the caller must treat the buffer as out of sync.
    pub fn buffer_edit(
        &self,
        path: String,
        expected_server_version: u64,
        new_client_version: u64,
        edits: Vec<crate::proto::TextEdit>,
    ) -> Result<(), super::BackendError> {
        match self.helper_for(Family::Buffers) {
            Some(client) => client
                .send_buffer_edit(path, expected_server_version, new_client_version, edits)
                .map_err(super::BackendError::from),
            None => Ok(()),
        }
    }

    /// Opens `path` in an editor buffer, whichever arm serves it, and makes sure the shell arm
    /// knows the file as it was loaded: a buffer the helper served is remembered (SHA-256 of the
    /// text) so that, if the helper goes away before the save, the save on the shell arm is
    /// guarded and reports a conflict rather than overwriting (#1865). This inherent method is
    /// the one call sites use; it shadows the trait method of the same name.
    pub async fn open_buffer(
        &self,
        path: String,
        force_reload: bool,
    ) -> Result<crate::proto::OpenBufferResponse, super::BackendError> {
        use super::RemoteFeatures;
        let before = self.shell.begin_open(&path);
        let result = RemoteFeatures::open_buffer(self, path.clone(), force_reload).await;
        self.shell.finish_open(
            &path,
            before,
            result.as_ref().ok().map(|opened| opened.content.as_str()),
        );
        result
    }

    /// Tells the host the editor closed `path`.
    pub fn close_buffer(&self, path: String) {
        match self.helper_for(Family::Buffers) {
            Some(client) => client.close_buffer(path),
            None => self.shell.close_buffer_shell(&path),
        }
    }

    /// Saves the editor's `content` of the open buffer `path`. With a helper the buffer is on the
    /// host already (the edits were sent as they were made), so this asks it to write it; with
    /// the shell serving buffers the text is written through a guarded write that refuses a file
    /// that changed since it was opened or last saved ([`BufferSaveOutcome::Conflict`]).
    /// Emits `AfterOwnWrite` on success.
    pub async fn save_buffer_content(
        &self,
        path: String,
        content: String,
    ) -> Result<BufferSaveOutcome, super::BackendError> {
        use super::RemoteFeatures as _;
        // A save through the buffer protocol is a file write too: say which arm took it, so a
        // run that wrote nothing cannot look like one that did (#1835).
        self.log_route(Family::FileWrite);
        let outcome = if self.helper_for(Family::Buffers).is_some() {
            use crate::proto::save_buffer_response::Result as SaveResult;
            match self.save_buffer(path).await?.result {
                Some(SaveResult::Error(e)) => BufferSaveOutcome::Failed(e.message),
                Some(SaveResult::Success(_)) | None => BufferSaveOutcome::Saved,
            }
        } else {
            self.shell.save_buffer_content(path, content).await?
        };
        if outcome == BufferSaveOutcome::Saved {
            self.router
                .emit_refresh(&self.host, &RemoteRefreshEvent::AfterOwnWrite);
        }
        Ok(outcome)
    }

    /// The helper client, if it is the one that serves `family` now.
    fn helper_for(&self, family: Family) -> Option<&Arc<RemoteServerClient>> {
        (self.kind(family) == BackendKind::Helper)
            .then(|| self.helper.as_ref())
            .flatten()
    }

    /// Asks for the git chip's status of `repo_path`. With the helper serving `GitStatus` it is
    /// told (`UpdateGitStatus`) and answers with a `GitStatusPush` on its connection, now and on
    /// every change; on the shell the status is computed now (`Computed(None)`: nothing to
    /// report) and the caller asks again on refresh events. `cached_main_branch` only matters to
    /// the shell arm.
    pub async fn request_git_status(
        &self,
        repo_path: &StandardizedPath,
        cached_main_branch: Option<String>,
    ) -> Result<Requested<Option<crate::proto::GitStatusPush>>, super::BackendError> {
        self.log_route(Family::GitStatus);
        if self.kind(Family::GitStatus) == BackendKind::Helper
            && let Some(client) = self.helper_client()
        {
            client.update_git_status(repo_path);
            return Ok(Requested::PushedByHelper);
        }
        self.shell
            .git_status_shell(&repo_path.to_string(), cached_main_branch)
            .await
            .map(Requested::Computed)
    }

    /// What a prompt chip's own command prints in `dir`, from the prompt's batched exec, when the
    /// shell serves the git chips of this host (no helper): see
    /// [`ShellBackend::batched_chip_shell`]. `Ok(None)`: not answered by a batch (a helper serves
    /// the host, or the batch is off), so the caller runs the chip's command as before.
    pub async fn batched_chip(
        &self,
        dir: &str,
        chip: super::BatchedChip,
    ) -> Result<Option<super::ChipCommandOutput>, super::BackendError> {
        if self.kind(Family::GitStatus) != BackendKind::Shell {
            return Ok(None);
        }
        self.shell.batched_chip_shell(dir, chip).await
    }

    /// Asks for PR info of `repo_path`'s current branch. The helper is told and pushes (it looks
    /// again by itself when the branch changes). On the shell it is looked up now: always with
    /// `force`, otherwise only when the branch differs from `known_branch`.
    pub async fn request_pr_info(
        &self,
        repo_path: &StandardizedPath,
        known_branch: Option<Option<String>>,
        force: bool,
    ) -> Result<Requested<PrLookup>, super::BackendError> {
        self.log_route(Family::Github);
        if self.kind(Family::Github) == BackendKind::Helper
            && let Some(client) = self.helper_client()
        {
            client.update_github_pr_info(repo_path);
            return Ok(Requested::PushedByHelper);
        }
        self.shell
            .pr_info_shell(&repo_path.to_string(), known_branch, force)
            .await
            .map(Requested::Computed)
    }

    /// Asks for the repository name / owner / host of `repo_path` (from `gh`); the helper pushes,
    /// the shell computes now.
    pub async fn request_repository_info(
        &self,
        repo_path: &StandardizedPath,
    ) -> Result<Requested<crate::proto::GitHubRepositoryInfoPush>, super::BackendError> {
        self.log_route(Family::Github);
        if self.kind(Family::Github) == BackendKind::Helper
            && let Some(client) = self.helper_client()
        {
            client.update_github_repo_info(repo_path);
            return Ok(Requested::PushedByHelper);
        }
        self.shell
            .repository_info_shell(&repo_path.to_string())
            .await
            .map(Requested::Computed)
    }

    /// Tells the helper (if it serves diff state) that the subscription for `(repo, mode)` is over.
    /// The shell arm holds no subscription, so there is nothing to end there.
    pub fn unsubscribe_diff_state(&self, request: crate::proto::UnsubscribeDiffState) {
        if self.kind(Family::DiffState) == BackendKind::Helper
            && let Some(client) = self.helper_client()
        {
            client.unsubscribe_diff_state(request);
        }
    }

    /// Whether a call for `family` could work at all right now: the helper serves it, or the
    /// shell has an out-of-band channel. (The shell may still not implement the family yet.)
    pub fn is_available(&self, family: Family) -> bool {
        self.kind(family) == BackendKind::Helper || self.shell.is_available()
    }
}

#[cfg(test)]
#[path = "router_tests.rs"]
mod tests;
