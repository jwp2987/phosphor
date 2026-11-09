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

use super::Family;
use super::executor::OutOfBandExecutor;
use super::helper::client_id;
use super::shell::{ShellBackend, ShellPush, TreeCache};
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
}

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
                .with_tree_cache(self.tree_cache_for(host)),
        }
    }

    /// The shell-mode file-tree state of `host` (created on first use).
    pub fn tree_cache_for(&self, host: &HostId) -> Arc<TreeCache> {
        Arc::clone(self.lock().tree_caches.entry(host.clone()).or_default())
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

    /// Whether a call for `family` could work at all right now: the helper serves it, or the
    /// shell has an out-of-band channel. (The shell may still not implement the family yet.)
    pub fn is_available(&self, family: Family) -> bool {
        self.kind(family) == BackendKind::Helper || self.shell.is_available()
    }
}

#[cfg(test)]
#[path = "router_tests.rs"]
mod tests;
