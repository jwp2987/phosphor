use super::MCPProvider;
use super::file_mcp_watcher::FileMCPConfigDiagnostic;
use super::{FileMCPWatcher, FileMCPWatcherEvent};
use itertools::Itertools as _;
use repo_metadata::repositories::DetectedRepositories;
use std::collections::{hash_map::Entry, HashMap, HashSet};
use std::path::{Path, PathBuf};
use uuid::Uuid;
use warp_core::execution_mode::AppExecutionMode;
use warp_core::features::FeatureFlag;
use warpui::{AppContext, Entity, ModelContext, SingletonEntity};

use crate::{
    ai::mcp::{
        templatable_installation::TemplatableMCPServerInstallation,
        ParsedTemplatableMCPServerResult,
    },
    settings::{ai::AISettings, AISettingsChangedEvent},
    warp_managed_paths_watcher::warp_managed_mcp_config_path,
};

/// Singleton model to manage file-based MCP servers.
#[derive(Default)]
pub struct FileBasedMCPManager {
    /// File-based MCP server installations detected from config files.
    /// Keyed by a consistent hash of the server's name, JSON template, and variable values.
    file_based_servers: HashMap<u64, TemplatableMCPServerInstallation>,
    /// Reverse mapping: logical root path → provider → set of server hashes.
    file_based_servers_by_root: HashMap<PathBuf, HashMap<MCPProvider, HashSet<u64>>>,
    /// The most recent diagnostic for each config file that failed to read or parse, keyed by
    /// config path. Cleared once that path parses (or is removed) successfully, so this only
    /// ever reflects the current state, not history.
    config_diagnostics_by_path: HashMap<PathBuf, FileMCPConfigDiagnostic>,
    /// The TUI scans its global config as soon as it starts so it can render
    /// config health immediately, but starting servers at scan time would expose
    /// tools (and begin any OAuth handshake) before the session is ready. Hold
    /// global Zap servers until the TUI front-end explicitly activates them, and
    /// never auto-start global third-party servers there at all — the TUI's MCP
    /// menu requires an explicit start action for those.
    ///
    /// Upstream derives this from `settings::settings_mode()`; this fork dropped
    /// `SettingsMode` (see `DECLINED.md`) and reads the equivalent runtime fact
    /// from [`AppExecutionMode::is_tui`].
    defer_global_warp_autostart: bool,
    /// Whether deferred global Zap servers may now be started. Always `true`
    /// outside the TUI, where nothing is deferred in the first place.
    global_warp_servers_activated: bool,
}

impl FileBasedMCPManager {
    pub fn new(ctx: &mut ModelContext<Self>) -> Self {
        let defer_global_warp_autostart = AppExecutionMode::as_ref(ctx).is_tui();
        if FeatureFlag::FileBasedMcp.is_enabled() {
            ctx.subscribe_to_model(&FileMCPWatcher::handle(ctx), |me, event, ctx| {
                me.handle_watcher_event(event, ctx);
            });

            ctx.subscribe_to_model(&AISettings::handle(ctx), |me, event, ctx| {
                if matches!(event, AISettingsChangedEvent::FileBasedMcpEnabled { .. }) {
                    me.handle_file_based_mcp_enabled_change(ctx);
                }
            });
        }

        Self {
            file_based_servers: Default::default(),
            file_based_servers_by_root: Default::default(),
            config_diagnostics_by_path: Default::default(),
            defer_global_warp_autostart,
            global_warp_servers_activated: !defer_global_warp_autostart,
        }
    }

    /// The most recent diagnostic for the config file at `config_path`, if it failed to read or
    /// parse. `None` once that path has parsed (or been removed) successfully.
    pub fn config_diagnostic(&self, config_path: &Path) -> Option<&FileMCPConfigDiagnostic> {
        self.config_diagnostics_by_path.get(config_path)
    }

    /// Owned snapshots of every current config diagnostic, in a stable order.
    ///
    /// The TUI reports one row per unhealthy config file rather than the single
    /// "is the one config file broken" answer `config_diagnostic` gives, because
    /// several providers' config files are read at once and any subset of them
    /// can be broken independently.
    #[cfg(any(feature = "tui", test))]
    pub fn config_diagnostics(&self) -> Vec<FileMCPConfigDiagnostic> {
        self.config_diagnostics_by_path
            .values()
            .cloned()
            .sorted_by(|left, right| {
                left.config_path.cmp(&right.config_path).then_with(|| {
                    provider_sort_key(left.provider).cmp(&provider_sort_key(right.provider))
                })
            })
            .collect()
    }

    /// Every file-based installation paired with all config sources that
    /// currently reference it, so a row can name each file it came from.
    ///
    /// Two providers can define the same server; the installation is deduped by
    /// content hash but its sources are not, which is what lets the menu say
    /// "Claude global, Codex · my-repo" on one row.
    #[cfg(any(feature = "tui", test))]
    pub fn file_based_servers_with_sources(&self) -> Vec<FileBasedMCPServerWithSources> {
        self.file_based_servers
            .iter()
            .sorted_by_key(|(hash, _)| **hash)
            .map(|(hash, installation)| {
                let mut sources = self
                    .file_based_servers_by_root
                    .iter()
                    .flat_map(|(root_path, provider_map)| {
                        provider_map
                            .iter()
                            .filter(|(_, hashes)| hashes.contains(hash))
                            .map(|(provider, _)| FileBasedMCPServerSource {
                                provider: *provider,
                                root_path: root_path.clone(),
                                scope: Self::scope_for_source(root_path, *provider),
                            })
                    })
                    .collect_vec();
                sources.sort_by(|left, right| {
                    left.root_path.cmp(&right.root_path).then_with(|| {
                        provider_sort_key(left.provider).cmp(&provider_sort_key(right.provider))
                    })
                });
                FileBasedMCPServerWithSources {
                    installation: installation.clone(),
                    sources,
                }
            })
            .collect()
    }

    /// Returns a file-based installation by its stable content hash.
    #[cfg(any(feature = "tui", test))]
    pub fn installation_by_hash(&self, hash: u64) -> Option<&TemplatableMCPServerInstallation> {
        self.file_based_servers.get(&hash)
    }

    /// Handle an event from [`FileMCPWatcher`].
    fn handle_watcher_event(&mut self, event: &FileMCPWatcherEvent, ctx: &mut ModelContext<Self>) {
        match event {
            FileMCPWatcherEvent::ConfigParsed {
                config_path,
                root_path,
                provider,
                servers,
            } => {
                if self
                    .config_diagnostics_by_path
                    .remove(config_path)
                    .is_some()
                {
                    ctx.emit(FileBasedMCPManagerEvent::ConfigDiagnosticChanged);
                }
                self.apply_parsed_servers(root_path.clone(), *provider, servers.clone(), ctx);
            }
            FileMCPWatcherEvent::ConfigRemoved {
                config_path,
                root_path,
                provider,
            } => {
                if self
                    .config_diagnostics_by_path
                    .remove(config_path)
                    .is_some()
                {
                    ctx.emit(FileBasedMCPManagerEvent::ConfigDiagnosticChanged);
                }
                self.remove_servers_for_root_provider(root_path, *provider, ctx);
            }
            FileMCPWatcherEvent::ConfigError { diagnostic } => {
                self.config_diagnostics_by_path
                    .insert(diagnostic.config_path.clone(), diagnostic.clone());
                ctx.emit(FileBasedMCPManagerEvent::ConfigDiagnosticChanged);
            }
        }
    }

    /// Get file-based MCP servers in scope for the given current working directory.
    pub fn get_servers_for_working_directory(
        &self,
        cwd: &Path,
        app: &AppContext,
    ) -> Vec<&TemplatableMCPServerInstallation> {
        let repo_root = DetectedRepositories::as_ref(app).get_root_for_path(cwd);
        let candidate_roots = [dirs::home_dir(), repo_root];

        let mut servers = Vec::new();
        for root in candidate_roots.into_iter().flatten() {
            // Get user and project-scoped MCP servers from all providers for the given cwd.
            if let Some(provider_map) = self.file_based_servers_by_root.get(&root) {
                for hash_set in provider_map.values() {
                    servers.extend(
                        hash_set
                            .iter()
                            .filter_map(|h| self.file_based_servers.get(h)),
                    );
                }
            }
        }
        servers
    }

    /// Removes all tracked servers for the given `(root_path, provider)` pair,
    /// then removes any that are no longer referenced elsewhere.
    fn remove_servers_for_root_provider(
        &mut self,
        root_path: &PathBuf,
        provider: MCPProvider,
        ctx: &mut ModelContext<Self>,
    ) {
        let hashes = self
            .file_based_servers_by_root
            .get_mut(root_path)
            .and_then(|m| m.remove(&provider));
        if let Some(hashes) = hashes {
            // Dropping an empty set leaves the effective source set unchanged, so
            // only a non-empty removal is a change worth announcing.
            let servers_changed = !hashes.is_empty();
            self.remove_if_orphaned(hashes, ctx);
            if servers_changed {
                ctx.emit(FileBasedMCPManagerEvent::ServersChanged);
            }
        }
    }

    /// Removes servers if they are no longer referenced by any (root_path, provider) pair.
    /// Orphaned servers are removed from `file_based_servers` and the templatable manager is
    /// notified to despawn them and purge their credentials.
    fn remove_if_orphaned(
        &mut self,
        hashes: impl IntoIterator<Item = u64>,
        ctx: &mut ModelContext<Self>,
    ) {
        let referenced_hashes: HashSet<u64> = self
            .file_based_servers_by_root
            .values()
            .flat_map(|provider_map| provider_map.values())
            .flat_map(|hash_set| hash_set.iter().copied())
            .collect();

        let removed_servers: Vec<_> = hashes
            .into_iter()
            .filter(|hash| !referenced_hashes.contains(hash))
            .filter_map(|hash| self.file_based_servers.remove(&hash))
            .collect();

        // Notify the templatable manager to remove orphaned servers and purge their credentials.
        if !removed_servers.is_empty() {
            let removed_uuids = removed_servers
                .iter()
                .map(|server| server.uuid())
                .collect_vec();
            ctx.emit(FileBasedMCPManagerEvent::DespawnServers {
                installation_uuids: removed_uuids,
            });

            let removed_hashes = removed_servers
                .iter()
                .filter_map(|server| server.hash())
                .collect_vec();
            ctx.emit(FileBasedMCPManagerEvent::PurgeCredentials {
                installation_hashes: removed_hashes,
            });
        }
    }

    /// Applies a parsed list of MCP servers
    /// spawning new servers and removing servers that are no longer present.
    fn apply_parsed_servers(
        &mut self,
        root_path: PathBuf,
        provider: MCPProvider,
        parsed_servers: Vec<ParsedTemplatableMCPServerResult>,
        ctx: &mut ModelContext<Self>,
    ) {
        let previous_scanned_servers: HashSet<u64> = self
            .file_based_servers_by_root
            .get(&root_path)
            .and_then(|m| m.get(&provider))
            .cloned()
            .unwrap_or_default();

        let mut servers_to_spawn = Vec::new();
        let mut scanned_servers = HashSet::new();
        for server in parsed_servers {
            let Some(installation) = server.templatable_mcp_server_installation else {
                continue;
            };
            let Some(hash) = installation.hash() else {
                continue;
            };
            // TODO(APP-3429): Deduplicate file-based servers across provider directories.
            if let Entry::Vacant(e) = self.file_based_servers.entry(hash) {
                // Detected a server that hasn't previously been spawned.
                // Initialize metadata and mark it for spawning.
                e.insert(installation.clone());
                servers_to_spawn.push(installation);
            }

            // In all cases, add a reference to the server in the (root_path, provider) entry.
            self.file_based_servers_by_root
                .entry(root_path.clone())
                .or_default()
                .entry(provider)
                .or_default()
                .insert(hash);
            scanned_servers.insert(hash);
        }

        // If file-based MCP is enabled, spawn any new servers.
        self.spawn_file_based_servers(servers_to_spawn, ctx);

        // Determine which servers have been removed.
        let servers_to_remove = previous_scanned_servers
            .difference(&scanned_servers)
            .copied()
            .collect_vec();

        // Remove any servers that are no longer present in the config file.
        if let Some(provider_map) = self.file_based_servers_by_root.get_mut(&root_path) {
            if let Some(hash_set) = provider_map.get_mut(&provider) {
                for hash in &servers_to_remove {
                    hash_set.remove(hash);
                }
            }

            // If the set of servers for the provider is empty, remove the provider from the map.
            if provider_map.get(&provider).is_some_and(|s| s.is_empty()) {
                provider_map.remove(&provider);
            }
        }

        // If the set of servers for the root path is empty, remove the root path from the map.
        if self
            .file_based_servers_by_root
            .get(&root_path)
            .is_some_and(|m| m.is_empty())
        {
            self.file_based_servers_by_root.remove(&root_path);
        }

        // If orphaned servers are found, remove them and purge their credentials.
        self.remove_if_orphaned(servers_to_remove, ctx);

        // Re-parsing a config that has not changed re-applies the identical hash
        // set; only a genuine change to this `(root, provider)`'s effective source
        // set is announced, so subscribers do not rebuild on every file event.
        if previous_scanned_servers != scanned_servers {
            ctx.emit(FileBasedMCPManagerEvent::ServersChanged);
        }
    }

    /// Returns `true` if the server identified by `hash` is referenced from any global
    /// config location.
    ///
    /// "Global" means the installation was detected outside of a user repository:
    /// - For `MCPProvider::Zap`: the logical root for `~/.warp*/.mcp.json`.
    /// - For any other provider: the user's home directory (e.g. `~/.claude.json`).
    ///
    /// Project-scoped installations (those detected inside a repo) are not considered
    /// global, even if they also happen to be referenced from a global location (in which
    /// case this returns `true` due to the global reference).
    fn is_global_server(&self, hash: u64) -> bool {
        self.file_based_servers_by_root
            .iter()
            .any(|(root_path, provider_map)| {
                provider_map.iter().any(|(provider, hashes)| {
                    hashes.contains(&hash)
                        && Self::scope_for_source(root_path, *provider)
                            == FileBasedMCPServerScope::Global
                })
            })
    }

    /// Returns `true` if the server identified by `hash` is referenced from the global
    /// Zap config (`~/.warp/.mcp.json`). Global Zap servers always auto-spawn.
    fn is_global_warp_server(&self, hash: u64) -> bool {
        self.file_based_servers_by_root
            .iter()
            .any(|(root_path, provider_map)| {
                Self::is_global_warp_root(root_path)
                    && provider_map
                        .get(&MCPProvider::Zap)
                        .is_some_and(|hashes| hashes.contains(&hash))
            })
    }

    fn is_global_warp_root(root_path: &Path) -> bool {
        warp_managed_mcp_config_path().is_some_and(|path| root_path == path.root_path.as_path())
    }

    /// Whether a `(root, provider)` config source is global or project-scoped.
    ///
    /// This is the single definition [`Self::is_global_server`] and the TUI
    /// catalog both read, so a row's rendered scope label can never disagree
    /// with the auto-start decision made for the same server.
    fn scope_for_source(root_path: &Path, provider: MCPProvider) -> FileBasedMCPServerScope {
        match provider {
            MCPProvider::Zap => {
                if Self::is_global_warp_root(root_path) {
                    FileBasedMCPServerScope::Global
                } else {
                    FileBasedMCPServerScope::Project
                }
            }
            MCPProvider::Claude | MCPProvider::Codex | MCPProvider::Agents => {
                if dirs::home_dir()
                    .as_ref()
                    .is_some_and(|home| root_path == home)
                {
                    FileBasedMCPServerScope::Global
                } else {
                    FileBasedMCPServerScope::Project
                }
            }
        }
    }

    fn spawn_file_based_servers(
        &mut self,
        servers_to_spawn: Vec<TemplatableMCPServerInstallation>,
        ctx: &mut ModelContext<Self>,
    ) {
        if servers_to_spawn.is_empty() {
            return;
        }
        let mcp_enabled = AISettings::as_ref(ctx).is_file_based_mcp_enabled(ctx);

        // Partition servers into three buckets based on scope:
        // - Global Zap: always auto-spawn.
        // - Global non-Zap: auto-spawn iff the toggle is on.
        // - Project-scoped (any provider): never auto-spawn; require explicit opt-in
        //   via the "Detected from {provider}" section of the MCP settings.
        let mut to_spawn = Vec::new();
        for installation in servers_to_spawn {
            let Some(hash) = installation.hash() else {
                continue;
            };
            let should_spawn = if self.is_global_warp_server(hash) {
                // Global Zap servers always auto-spawn, except in the TUI before
                // activation (see `defer_global_warp_autostart`).
                !self.defer_global_warp_autostart || self.global_warp_servers_activated
            } else if self.is_global_server(hash) {
                // Global third-party servers follow the GUI toggle, and never
                // auto-start in the TUI — that guarantee is unconditional, not
                // merely "until activation".
                !self.defer_global_warp_autostart && mcp_enabled
            } else {
                // Project-scoped installations are intentionally dropped from auto-spawn.
                false
            };
            if should_spawn {
                to_spawn.push(installation);
            }
        }

        if !to_spawn.is_empty() {
            ctx.emit(FileBasedMCPManagerEvent::SpawnServers {
                installations: to_spawn,
            });
        }
    }

    fn handle_file_based_mcp_enabled_change(&mut self, ctx: &mut ModelContext<Self>) {
        // The setting is GUI-only. TUI-discovered third-party servers always
        // require an explicit start action, even if a value is loaded into the
        // shared model by tests or future settings migrations.
        if self.defer_global_warp_autostart {
            return;
        }
        // Only global third-party servers are affected by the toggle:
        // - Global Zap servers always spawn regardless of the toggle.
        // - Project-scoped servers (any provider) are never auto-spawned and their
        //   running state is managed per-card via the MCP settings UI; toggling the
        //   setting must not spawn or despawn them.
        let global_third_party_servers: Vec<_> = self
            .file_based_servers
            .iter()
            .filter(|(hash, _)| {
                self.is_global_server(**hash) && !self.is_global_warp_server(**hash)
            })
            .map(|(_, server)| server.clone())
            .collect();
        if !AISettings::as_ref(ctx).is_file_based_mcp_enabled(ctx) {
            // Toggle off: despawn global third-party servers only.
            ctx.emit(FileBasedMCPManagerEvent::DespawnServers {
                installation_uuids: global_third_party_servers
                    .iter()
                    .map(|s| s.uuid())
                    .collect_vec(),
            });
        } else {
            // Toggle on: spawn global third-party servers (global Zap servers are
            // already running; project-scoped servers are unaffected).
            ctx.emit(FileBasedMCPManagerEvent::SpawnServers {
                installations: global_third_party_servers,
            });
        }
    }

    /// Every installation referenced from the global Zap config (`~/.warp/mcp.json`).
    #[cfg(any(feature = "tui", test))]
    pub fn global_warp_servers(&self) -> Vec<&TemplatableMCPServerInstallation> {
        self.file_based_servers
            .iter()
            .filter(|(hash, _)| self.is_global_warp_server(**hash))
            .map(|(_, installation)| installation)
            .collect()
    }

    /// Releases the TUI's deferred global Zap servers and starts the ones already
    /// detected. Idempotent, and a no-op outside the TUI (nothing was deferred).
    ///
    /// Global third-party servers are deliberately *not* released here: in the TUI
    /// they always require an explicit start action from the MCP menu.
    #[cfg(any(feature = "tui", test))]
    pub fn activate_global_warp_servers(&mut self, ctx: &mut ModelContext<Self>) {
        if self.global_warp_servers_activated {
            return;
        }
        self.global_warp_servers_activated = true;
        let installations = self
            .global_warp_servers()
            .into_iter()
            .cloned()
            .collect_vec();
        if !installations.is_empty() {
            ctx.emit(FileBasedMCPManagerEvent::SpawnServers { installations });
        }
    }

    pub fn get_hash_by_uuid(&self, installation_uuid: Uuid) -> Option<u64> {
        self.file_based_servers
            .iter()
            .find(|(_, server)| server.uuid() == installation_uuid)
            .map(|(hash, _)| *hash)
    }

    /// Returns all detected file-based MCP server installations.
    pub fn file_based_servers(&self) -> Vec<&TemplatableMCPServerInstallation> {
        self.file_based_servers.values().collect()
    }

    /// Returns the installation with the given UUID, if any.
    pub fn get_installation_by_uuid(
        &self,
        uuid: Uuid,
    ) -> Option<&TemplatableMCPServerInstallation> {
        self.file_based_servers
            .values()
            .find(|server| server.uuid() == uuid)
    }

    /// Returns all root paths for the given installation scoped to a specific provider.
    pub fn directory_paths_for_installation_and_provider(
        &self,
        uuid: Uuid,
        provider: MCPProvider,
    ) -> Vec<PathBuf> {
        let Some(hash) = self.get_hash_by_uuid(uuid) else {
            return vec![];
        };
        self.file_based_servers_by_root
            .iter()
            .filter(|(_, provider_map)| {
                provider_map
                    .get(&provider)
                    .is_some_and(|hashes| hashes.contains(&hash))
            })
            .map(|(root, _)| root.clone())
            .sorted()
            .collect()
    }

    /// Returns the directory a file-based MCP installation should be spawned from
    /// when its config does not specify `working_directory`.
    ///
    /// The spawn root is the directory the config was discovered in, with one
    /// exception: global Zap installs are discovered in `~/.warp*/`, which
    /// isn't a useful cwd for spawned processes, so they are remapped to the
    /// home directory instead.
    /// - Project-scoped installations: the repo root.
    /// - Global installations (`~/.warp/.mcp.json`, `~/.claude.json`, etc.): the
    ///   home directory.
    ///
    /// If the installation is referenced from multiple roots, the lexicographically
    /// smallest is returned for determinism. Returns `None` for installations that
    /// are not tracked by `FileBasedMCPManager` (e.g. cloud-templated installs).
    pub fn spawn_root_for_installation(&self, uuid: Uuid) -> Option<PathBuf> {
        let hash = self.get_hash_by_uuid(uuid)?;
        let discovery_root = self
            .file_based_servers_by_root
            .iter()
            .filter(|(_, provider_map)| provider_map.values().any(|hashes| hashes.contains(&hash)))
            .map(|(root, _)| root.clone())
            .sorted()
            .next()?;

        // Global Zap installs live under `~/.warp*/`, which is internal Zap
        // state rather than a meaningful working directory. Map them to the
        // home dir so all global installs (Zap and third-party) share a
        // consistent cwd.
        if self.is_global_warp_server(hash) {
            return dirs::home_dir().or(Some(discovery_root));
        }
        Some(discovery_root)
    }
}

#[cfg(any(feature = "tui", test))]
fn provider_sort_key(provider: MCPProvider) -> u8 {
    match provider {
        MCPProvider::Zap => 0,
        MCPProvider::Claude => 1,
        MCPProvider::Codex => 2,
        MCPProvider::Agents => 3,
    }
}

/// Whether a file-based config source is a global one (the user's home dir, or
/// the managed Zap config root) or a project one (inside a repository).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileBasedMCPServerScope {
    Global,
    Project,
}

/// One config file that defines a file-based server.
#[cfg(any(feature = "tui", test))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileBasedMCPServerSource {
    pub provider: MCPProvider,
    pub root_path: PathBuf,
    pub scope: FileBasedMCPServerScope,
}

/// A file-based installation together with every config source defining it.
#[cfg(any(feature = "tui", test))]
#[derive(Clone, Debug)]
pub struct FileBasedMCPServerWithSources {
    pub installation: TemplatableMCPServerInstallation,
    pub sources: Vec<FileBasedMCPServerSource>,
}

pub enum FileBasedMCPManagerEvent {
    /// The effective set of `(root, provider)` server references changed — a config
    /// introduced or dropped servers. Deliberately *not* emitted when a re-parse
    /// yields the same set, so subscribers can treat it as "the catalog moved".
    ServersChanged,
    /// A config file's diagnostic (read/parse/missing-env-var error, or its resolution) changed.
    ConfigDiagnosticChanged,
    SpawnServers {
        installations: Vec<TemplatableMCPServerInstallation>,
    },
    DespawnServers {
        installation_uuids: Vec<Uuid>,
    },
    PurgeCredentials {
        installation_hashes: Vec<u64>,
    },
}

impl Entity for FileBasedMCPManager {
    type Event = FileBasedMCPManagerEvent;
}

impl SingletonEntity for FileBasedMCPManager {}

#[cfg(test)]
#[path = "file_based_manager_tests.rs"]
mod tests;
