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
use super::Family;
use super::executor::OutOfBandExecutor;
use super::helper::client_id;
use super::refresh::RemoteRefreshEvent;
use super::shell::{ShellBackend, ShellBuffers, ShellPush, TreeCache};
use crate::HostId;
use crate::capability::ServerCapabilities;
use crate::client::RemoteServerClient;

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
    /// Per-host shell-mode buffer states (family c), shared likewise.
    buffer_states: HashMap<HostId, Arc<ShellBuffers>>,
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
        self.lock().executors.remove(&session);
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
        RoutedBackend {
            router: self.clone(),
            host: host.clone(),
            helper,
            shell: ShellBackend::new(self.executor_for(sessions))
                .with_tree_cache(self.tree_cache_for(host))
                .with_buffers(self.buffers_for(host)),
        }
    }

    /// The shell-mode file-tree state of `host` (created on first use).
    pub fn tree_cache_for(&self, host: &HostId) -> Arc<TreeCache> {
        Arc::clone(self.lock().tree_caches.entry(host.clone()).or_default())
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

    /// [`RemoteFeatures::write_file`](super::RemoteFeatures::write_file) that, when `expect` is
    /// given, writes only if the file is still in that state. The check needs the shell arm: a
    /// helper write has no precondition (the helper's conflict detection is its buffer
    /// protocol), so with a helper serving the family the write goes through unguarded and the
    /// caller relies on that protocol. Emits `AfterOwnWrite` like any write.
    pub async fn write_file_expecting(
        &self,
        path: String,
        content: String,
        expect: Option<crate::shellcmd::write::Expect>,
    ) -> Result<(), super::BackendError> {
        use super::RemoteFeatures as _;
        if expect.is_none() || self.kind(Family::FileWrite) == BackendKind::Helper {
            return self.write_file(path, content).await;
        }
        let result = self.shell.write_file_expecting(path, content, expect).await;
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

    /// Whether a call for `family` could work at all right now: the helper serves it, or the
    /// shell has an out-of-band channel. (The shell may still not implement the family yet.)
    pub fn is_available(&self, family: Family) -> bool {
        self.kind(family) == BackendKind::Helper || self.shell.is_available()
    }
}

#[cfg(test)]
#[path = "router_tests.rs"]
mod tests;
