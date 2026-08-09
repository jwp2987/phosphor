//! The live skill index: file-backed skills discovered by [`SkillWatcher`], bundled
//! skills (local + connected remote hosts), and the fork-original skill *inventory*
//! surface (`list_skill_inventory` / [`SkillInventoryItem`] / [`SkillInventoryDuplicate`])
//! consumed by `app/src/skill_manager/panel.rs`.
//!
//! This file is a **merge**, not a straight port of the pin's `ai/skills/skill_manager.rs`
//! (`02b53fcd8`): the inventory surface above has no pin equivalent (the pin's
//! `SkillManagerEvent` is a bare `SkillsChanged { home_skills_changed }`, not
//! `InventoryChanged`, and the pin has no `list_skill_inventory` at all) and must be
//! preserved intact per AGENTS.md §5.10 while merging in the pin's `SkillPathOrigin`-aware
//! remote-catalog plumbing (`BundledSkills`, `remote_home_directories`,
//! `*_with_origin` methods). Call sites of the inventory surface (recorded so nobody
//! re-derives this by grep): `app/src/skill_manager/panel.rs` (`list_skill_inventory` ×4 at
//! `new`/`update_inventory`/`filtered_items`/render path, `SkillInventoryItem`,
//! `SkillInventoryDuplicate`, `SkillManagerEvent` via `crate::ai::skills`).
//!
//! `is_cloud_environment` / `SkillManager::set_cloud_environment` from the pin are **not**
//! ported: they widen skill scope to every directory in `directory_skills` regardless of
//! cwd, for a Warp cloud-runner conversation with configured repos. Phosphor has no cloud
//! runner (see `DECLINED.md`'s RunAgents/orchestration rows), so that branch and its
//! `path_matches_location` helper have no non-cloud caller here; only the ordinary
//! "skills under the working directory's ancestors" branch is ported.

#[path = "file_watchers/mod.rs"]
mod file_watchers;
pub use file_watchers::{extract_skill_parent_directory, SkillWatcher, SkillWatcherEvent};

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use super::bundled::{BundledSkill, BundledSkills};
pub use super::bundled::BundledSkillActivation;
#[cfg(test)]
use super::bundled::{build_bundled_skill_context, read_bundled_skills};
use super::{
    ActiveSkillLookupError, SkillDescriptor, SkillInventoryDuplicate, SkillInventoryItem,
    SkillManagerEvent, SkillPathQuery,
};
use crate::ai::skills::skill_utils::SkillDeduplicator;
use ai::skills::{
    get_provider_for_path, provider_rank, ParsedSkill, SkillPathOrigin, SkillProvider,
    SkillReference,
};
use warp_core::features::FeatureFlag;
use warp_util::host_id::HostId;
use warp_util::local_or_remote_path::LocalOrRemotePath;
use warpui::{AppContext, Entity, ModelContext, ModelHandle, SingletonEntity};

pub struct SkillManager {
    /// Maps a directory path to the set of skill file paths defined in that directory.
    ///
    /// The key is the directory containing the `.agents/skills/` (or similar provider) folder,
    /// not the skills folder itself.
    ///
    /// Example: For a skill at `/repo/frontend/.agents/skills/deploy/SKILL.md`:
    /// - Key: `/repo/frontend`
    /// - Value (in the set): `/repo/frontend/.agents/skills/deploy/SKILL.md`
    ///
    /// NOT:
    /// - Key: `/repo/frontend/.agents/skills`
    directory_skills: HashMap<LocalOrRemotePath, HashSet<LocalOrRemotePath>>,
    skills_by_path: HashMap<LocalOrRemotePath, ParsedSkill>,
    /// Reverse lookup: skill name → set of paths with that name.
    /// This allows efficient lookup by skill name without scanning all paths.
    skills_by_name: HashMap<String, HashSet<LocalOrRemotePath>>,
    /// Skills bundled into Zap for the local host and connected remote hosts.
    bundled_skills: BundledSkills,
    /// Home directories published by connected remote hosts.
    ///
    /// Remote home skills themselves live in the shared file-skill indexes above,
    /// alongside local home and project skills.
    remote_home_directories: HashMap<HostId, LocalOrRemotePath>,
    #[allow(dead_code)]
    skill_watcher: ModelHandle<SkillWatcher>, // Can't remove this or it'll get cleaned up after new()
}

impl SkillManager {
    pub fn new(ctx: &mut ModelContext<Self>) -> Self {
        let (skill_watcher_tx, skill_watcher_rx) = async_channel::unbounded();

        ctx.spawn_stream_local(
            skill_watcher_rx,
            |me, message, ctx| {
                me.handle_skill_watcher_event(message, ctx);
            },
            |_, _| {}, // No cleanup needed when stream ends
        );

        // Create skill watcher
        let skill_watcher = ctx.add_model(|ctx| SkillWatcher::new(ctx, skill_watcher_tx));

        if FeatureFlag::BundledSkills.is_enabled() {
            ctx.spawn(BundledSkill::detect(), |me, result, _| {
                me.bundled_skills.set_local(result);
            });
        }

        Self {
            directory_skills: HashMap::new(),
            skills_by_path: HashMap::new(),
            skills_by_name: HashMap::new(),
            bundled_skills: BundledSkills::default(),
            remote_home_directories: HashMap::new(),
            skill_watcher,
        }
    }

    /// Returns skills available for the given working directory.
    pub fn get_skills_for_working_directory(
        &self,
        working_directory: Option<&LocalOrRemotePath>,
        ctx: &AppContext,
    ) -> Vec<SkillDescriptor> {
        let path_origin = match working_directory {
            Some(LocalOrRemotePath::Remote(path)) => SkillPathOrigin::Remote {
                host_id: path.host_id.clone(),
            },
            Some(LocalOrRemotePath::Local(_)) | None => SkillPathOrigin::Local,
        };
        self.get_skills_for_working_directory_with_origin(working_directory, &path_origin, ctx)
    }

    /// Returns skills available for the given working directory and execution host.
    pub fn get_skills_for_working_directory_with_origin(
        &self,
        working_directory: Option<&LocalOrRemotePath>,
        path_origin: &SkillPathOrigin,
        ctx: &AppContext,
    ) -> Vec<SkillDescriptor> {
        // Collect file-backed skills for one shared deduplication pass. Home skills use
        // the home directory as their dir_path; project skills use their owning directory.
        let mut skill_paths = Vec::new();
        let mut deduplicator = SkillDeduplicator::default();

        if let Some(home_dir) = self.home_directory_for_origin(path_origin)
            && let Some(home_skill_paths) = self.directory_skills.get(&home_dir)
        {
            skill_paths.extend(
                home_skill_paths
                    .iter()
                    .cloned()
                    .map(|path| (home_dir.clone(), path)),
            );
        }

        if let Some(working_directory) = working_directory {
            let repo_root = repo_metadata::repositories::DetectedRepositories::as_ref(ctx)
                .get_root_for_lor_path(working_directory);

            for (dir, dir_skill_paths) in &self.directory_skills {
                if self.is_home_directory(dir) {
                    continue;
                }
                // Only include skills from directories that are ancestors of the working directory
                // (or the working directory itself)
                if working_directory.starts_with(dir) {
                    // Also verify this directory is within the detected repo (if any)
                    if repo_root.as_ref().is_none_or(|root| dir.starts_with(root)) {
                        for path in dir_skill_paths {
                            skill_paths.push((dir.clone(), path.clone()));
                        }
                    }
                }
            }
        }

        // Deduplicate skills with identical content installed under the same directory across
        // multiple providers, keeping the skill from the highest-priority provider per
        // [`SKILL_PROVIDER_DEFINITIONS`].
        deduplicator.extend_paths(&skill_paths, &self.skills_by_path);
        let mut skills = deduplicator.into_descriptors();

        // Apply icon overrides for well-known skill names (e.g. partner integrations).
        for skill in &mut skills {
            if skill.icon_override.is_none() {
                skill.icon_override =
                    crate::ai::skills::skill_utils::icon_override_for_skill_name(&skill.name);
            }
        }

        // Append bundled skills whose activation condition is met, from the catalog of
        // the active execution host: SSH sessions see the remote daemon's catalog
        // (populated by `RemoteAgentContext` as its snapshots arrive; empty until then),
        // never the local client's. Remote catalog descriptors are referenced by their
        // remote paths so invocation resolves back to the same host's catalog, while
        // direct `BundledSkillId` lookups use `path_origin`.
        if FeatureFlag::BundledSkills.is_enabled() {
            skills.extend(self.bundled_skills.active_descriptors(path_origin, ctx));
        }

        skills
    }

    /// Returns the currently-known home skill file paths.
    pub fn home_skill_paths(&self) -> Vec<LocalOrRemotePath> {
        let Some(home_dir) = self.home_directory_for_origin(&SkillPathOrigin::Local) else {
            return vec![];
        };
        self.directory_skills
            .get(&home_dir)
            .map(|skills| skills.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Returns the parsed home skills currently cached by the local watcher.
    ///
    /// Used by the remote-server daemon (`app/src/remote_server/server_model.rs`) to
    /// serialize its own home skills into the `RemoteAgentContextSnapshot` it pushes to
    /// connected clients (#353) — the daemon runs the same `SkillManager`/`SkillWatcher`
    /// as a local client, just against its own host's home directory.
    pub fn home_skills(&self) -> impl Iterator<Item = &ParsedSkill> + '_ {
        dirs::home_dir()
            .map(LocalOrRemotePath::Local)
            .into_iter()
            .filter_map(|home_dir| self.directory_skills.get(&home_dir))
            .flatten()
            .filter_map(|path| self.skills_by_path.get(path))
    }

    /// Returns the currently-known directories which have skills registered.
    /// This includes both repo roots and subdirectories with skills.
    pub fn directories_with_skills(&self) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = self
            .directory_skills
            .keys()
            .filter_map(|path| path.to_local_path().map(Path::to_path_buf))
            .collect();
        dirs.sort();
        dirs
    }

    /// Returns skill file paths that are under `scope_dir`.
    ///
    /// This is used for skill resolution when the agent is invoked in a directory
    /// above a series of repos—we need skills in those repos to be in scope.
    ///
    /// Example: If `scope_dir` is `/code` and there are skills at:
    /// - `/code/repo-a/.agents/skills/deploy/SKILL.md`
    /// - `/code/repo-b/.agents/skills/test/SKILL.md`
    /// Both will be returned.
    pub fn skill_paths_in_scope(&self, scope_dir: &Path) -> Vec<PathBuf> {
        let mut paths = HashSet::new();
        let scope_dir = LocalOrRemotePath::Local(scope_dir.to_path_buf());

        for (dir, skill_paths) in &self.directory_skills {
            // Include skills from directories that are under scope_dir
            if dir.starts_with(&scope_dir) {
                paths.extend(
                    skill_paths
                        .iter()
                        .filter_map(|path| path.to_local_path().map(Path::to_path_buf)),
                );
            }
        }

        let mut paths: Vec<PathBuf> = paths.into_iter().collect();
        paths.sort();
        paths
    }

    /// Returns true if the skill (or any of its provider-path variants) exists in
    /// a folder matching one of the given `providers`. This handles the deduplication
    /// edge case where a skill is present in multiple provider folders (e.g. both
    /// `.agents/skills/` and `.claude/skills/`) but deduplication picked a provider
    /// that the caller doesn't support.
    pub fn skill_exists_for_any_provider(
        &self,
        skill: &SkillDescriptor,
        providers: &[SkillProvider],
    ) -> bool {
        // Fast path: the deduplicated provider already matches.
        if providers.contains(&skill.provider) {
            return true;
        }
        // Slow path: check all paths for this skill name.
        self.providers_for_descriptor(skill)
            .any(|provider| providers.contains(&provider))
    }

    /// Returns the best supported provider for a skill given a set of supported providers.
    ///
    /// When a skill is duplicated across multiple provider folders (e.g. both
    /// `.agents/skills/` and `.claude/skills/`), the global deduplication picks the
    /// highest-priority provider per [`SKILL_PROVIDER_DEFINITIONS`]. However, for the
    /// CLI agent footer `/skills` menu we want the icon to reflect the provider that
    /// the active CLI agent actually supports.
    ///
    /// This method checks all paths for the skill name and returns the supported
    /// provider with the best (lowest) rank. Falls back to the skill's deduped
    /// provider if no supported provider is found among its paths.
    pub fn best_supported_provider(
        &self,
        skill: &SkillDescriptor,
        supported_providers: &[SkillProvider],
    ) -> SkillProvider {
        // Fast path: the deduplicated provider is already supported.
        if supported_providers.contains(&skill.provider) {
            return skill.provider;
        }
        // Find the supported provider with the best (lowest) rank among all paths.
        self.providers_for_descriptor(skill)
            .filter(|provider| supported_providers.contains(provider))
            .min_by_key(|provider| provider_rank(*provider))
            .unwrap_or(skill.provider)
    }

    /// Iterates the providers of every indexed path sharing `descriptor`'s name and the
    /// same local/remote host as `descriptor`'s own reference (so a remote skill's
    /// duplicate-provider search never crosses into a same-named local or
    /// different-host skill).
    fn providers_for_descriptor<'a>(
        &'a self,
        descriptor: &'a SkillDescriptor,
    ) -> impl Iterator<Item = SkillProvider> + 'a {
        self.skills_by_name
            .get(&descriptor.name)
            .into_iter()
            .flatten()
            .filter(|path| path_matches_reference_location(path, &descriptor.reference))
            .filter_map(|path| self.skills_by_path.get(path).map(|skill| skill.provider))
    }

    /// Returns skill file paths that have the given skill name.
    /// A skill's name comes from the `name` field in its SKILL.md front matter.
    pub fn skill_paths_by_name(&self, name: &str) -> Vec<LocalOrRemotePath> {
        self.skills_by_name
            .get(name)
            .map(|paths| {
                let mut paths: Vec<LocalOrRemotePath> = paths.iter().cloned().collect();
                paths.sort_by_key(LocalOrRemotePath::display_path);
                paths
            })
            .unwrap_or_default()
    }

    /// Returns a reference to a parsed skill for a specific SKILL.md file path, if it is
    /// cached. Falls through to the remote bundled catalog, whose skills are addressed
    /// by path.
    pub fn skill_by_path<P: SkillPathQuery + ?Sized>(
        &self,
        skill_path: &P,
    ) -> Option<&ParsedSkill> {
        let location = skill_path.to_skill_location();
        self.skill_by_location(&location)
    }

    /// Returns the appropriate `SkillReference` for a skill at the given path.
    /// For bundled skills, returns `BundledSkillId`; otherwise returns `Path`.
    pub fn reference_for_skill_path<P: SkillPathQuery + ?Sized>(
        &self,
        skill_path: &P,
    ) -> SkillReference {
        let location = skill_path.to_skill_location();
        // Check if this path belongs to a bundled skill.
        if let Some(reference) = self.bundled_skills.reference_for_path(&location) {
            return reference;
        }
        // Default to path-based reference.
        SkillReference::Path(location)
    }

    /// Get the definition of a skill, if it is cached.
    pub fn skill_by_reference(&self, reference: &SkillReference) -> Option<&ParsedSkill> {
        match reference {
            SkillReference::Path(path) => self.skill_by_location(path),
            SkillReference::BundledSkillId(id) => self.bundled_skills.local_skill(id),
        }
    }

    /// Looks up the best match by skill name (the SKILL.md frontmatter `name` field).
    ///
    /// Used by the BYOP `read_skill` tool: the model can only see `<name>` in the
    /// system prompt and doesn't know SKILL.md's absolute path, so name →
    /// ParsedSkill resolution must be supported.
    ///
    /// When multiple copies share the same name, picks the first by ascending
    /// [`provider_rank`] (`Agents > Zap > Claude > …`), keeping priority consistent
    /// with [`SkillDeduplicator`]/`list_skill_inventory`. Bundled skills don't enter the
    /// `skills_by_name` index, so the local bundled catalog is searched separately as a
    /// fallback (fork-original; no pin equivalent — see this file's module doc comment).
    ///
    /// Local-only (the BYOP `read_skill` tool's cache-miss fallback this backs only
    /// ever reads from the local filesystem — see `read_skill.rs`): a remote skill
    /// sharing `name` is never returned here, even when connected to exactly one
    /// remote host.
    pub fn find_skill_by_name(&self, name: &str) -> Option<&ParsedSkill> {
        // Prefers filesystem skills: when multiple copies share a name, picks the best by provider_rank.
        let best_fs_path = self
            .skill_paths_by_name(name)
            .into_iter()
            .filter_map(|path| path.to_local_path().map(Path::to_path_buf))
            .min_by_key(|path| {
                get_provider_for_path(&LocalOrRemotePath::Local(path.clone()))
                    .map(provider_rank)
                    .unwrap_or(usize::MAX)
            });
        if let Some(path) = best_fs_path {
            if let Some(skill) = self.skills_by_path.get(&LocalOrRemotePath::Local(path)) {
                return Some(skill);
            }
        }
        // Fallback: local bundled skills (matched by name rather than id).
        self.bundled_skills
            .local_definitions()
            .map(|(_, skill)| skill)
            .find(|skill| skill.name == name)
    }

    /// Returns a local bundled skill by ID only if its activation condition is met.
    pub fn active_local_bundled_skill(&self, id: &str, ctx: &AppContext) -> Option<&ParsedSkill> {
        self.bundled_skills
            .active_skill(id, &SkillPathOrigin::Local, ctx)
    }

    /// Get the definition of a skill only if it is currently available for invocation.
    ///
    /// Path-based user skills are always controlled by normal path scoping. Bundled
    /// skills additionally respect their runtime activation state, so a stale
    /// `BundledSkillId` reference (e.g. copied from an earlier response, or for a
    /// skill whose activation condition has since flipped off) cannot invoke a
    /// disabled bundled skill. Equivalent to always passing `SkillPathOrigin::Local` to
    /// [`Self::active_skill_by_reference_with_origin`].
    pub fn active_skill_by_reference(
        &self,
        reference: &SkillReference,
        ctx: &AppContext,
    ) -> Option<&ParsedSkill> {
        self.active_skill_by_reference_with_origin(reference, &SkillPathOrigin::Local, ctx)
            .ok()
    }

    /// Get the definition of a skill for the selected execution host only if it is active.
    pub fn active_skill_by_reference_with_origin(
        &self,
        reference: &SkillReference,
        path_origin: &SkillPathOrigin,
        ctx: &AppContext,
    ) -> Result<&ParsedSkill, ActiveSkillLookupError> {
        let skill = match reference {
            SkillReference::Path(path) => self.skills_by_path.get(path).or_else(|| {
                let remote = path.as_remote()?;
                let SkillPathOrigin::Remote { host_id } = path_origin else {
                    return None;
                };
                if remote.host_id != *host_id {
                    return None;
                }
                self.bundled_skills.remote_active_skill_by_path(remote, ctx)
            }),
            SkillReference::BundledSkillId(id) => {
                self.bundled_skills.active_skill(id, path_origin, ctx)
            }
        };
        skill.ok_or_else(|| ActiveSkillLookupError::for_reference(reference, path_origin))
    }

    pub fn list_skill_inventory(&self, ctx: &AppContext) -> Vec<SkillInventoryItem> {
        let _ = ctx;
        let mut by_name: HashMap<String, Vec<SkillInventoryDuplicate>> = HashMap::new();

        for skill in self.skills_by_path.values() {
            // The inventory panel is local-only UI; skills without a local path
            // representation (e.g. a remote host's home skills, reconciled by
            // `RemoteAgentContext`) are skipped rather than mis-displayed.
            let Some(path) = skill.path.to_local_path().map(Path::to_path_buf) else {
                continue;
            };
            by_name
                .entry(skill.name.clone())
                .or_default()
                .push(SkillInventoryDuplicate {
                    path,
                    name: skill.name.clone(),
                    description: skill.description.clone(),
                    content: skill.content.clone(),
                    provider: skill.provider,
                    scope: skill.scope,
                });
        }

        let mut items = by_name
            .into_iter()
            .filter_map(|(name, mut duplicates)| {
                duplicates.sort_by(|a, b| {
                    provider_rank(a.provider)
                        .cmp(&provider_rank(b.provider))
                        .then_with(|| format!("{:?}", a.scope).cmp(&format!("{:?}", b.scope)))
                        .then_with(|| a.path.cmp(&b.path))
                });
                let default_skill = duplicates.first()?.clone();
                Some(SkillInventoryItem {
                    name,
                    default_skill,
                    duplicates,
                })
            })
            .collect::<Vec<_>>();

        items.sort_by(|a, b| a.name.cmp(&b.name));
        items
    }

    // ── Remote Agent Mode context reconciliation (#353/#487) ────────────────────
    //
    // Called by `RemoteAgentContext` (`app/src/ai/remote_agent_context.rs`) as
    // `RemoteAgentContextSnapshot`s arrive from connected SSH hosts and as hosts
    // disconnect.

    pub(super) fn set_remote_bundled_skill(&mut self, host_id: HostId, bundled_skill: BundledSkill) {
        self.bundled_skills.insert_remote(host_id, bundled_skill);
    }

    pub(super) fn remove_remote_bundled_skill(&mut self, host_id: &HostId) {
        self.bundled_skills.remove_remote(host_id);
    }

    pub(crate) fn replace_remote_agent_context(
        &mut self,
        host_id: HostId,
        bundled_skills: Option<BundledSkill>,
        home_skills: Option<(LocalOrRemotePath, Vec<ParsedSkill>)>,
        ctx: &mut ModelContext<Self>,
    ) {
        match bundled_skills {
            Some(bundled_skills) => {
                self.set_remote_bundled_skill(host_id.clone(), bundled_skills);
            }
            None => self.remove_remote_bundled_skill(&host_id),
        }
        match home_skills {
            Some((home_dir, skills)) => {
                self.set_remote_home_skills(host_id, home_dir, skills, ctx);
            }
            None => self.remove_remote_home_skills(&host_id, ctx),
        }
    }

    pub(crate) fn remove_remote_agent_context(
        &mut self,
        host_id: &HostId,
        ctx: &mut ModelContext<Self>,
    ) {
        self.remove_remote_bundled_skill(host_id);
        self.remove_remote_home_skills(host_id, ctx);
    }

    /// Replaces the home skills published by one remote host.
    pub(crate) fn set_remote_home_skills(
        &mut self,
        host_id: HostId,
        home_dir: LocalOrRemotePath,
        skills: Vec<ParsedSkill>,
        ctx: &mut ModelContext<Self>,
    ) {
        self.remove_remote_home_skills(&host_id, ctx);
        self.remote_home_directories.insert(host_id, home_dir);
        self.handle_skills_added(skills, ctx);
    }

    pub(crate) fn remove_remote_home_skills(
        &mut self,
        host_id: &HostId,
        ctx: &mut ModelContext<Self>,
    ) {
        let Some(home_dir) = self.remote_home_directories.remove(host_id) else {
            return;
        };
        self.remove_skills_for_directory(&home_dir);
        ctx.emit(SkillManagerEvent::InventoryChanged);
    }

    fn home_directory_for_origin(&self, path_origin: &SkillPathOrigin) -> Option<LocalOrRemotePath> {
        match path_origin {
            SkillPathOrigin::Local => dirs::home_dir().map(LocalOrRemotePath::Local),
            SkillPathOrigin::Remote { host_id } => {
                self.remote_home_directories.get(host_id).cloned()
            }
            SkillPathOrigin::RestoredDisplayOnly | SkillPathOrigin::Unavailable => None,
        }
    }

    fn is_home_directory(&self, path: &LocalOrRemotePath) -> bool {
        match path {
            LocalOrRemotePath::Local(path) => dirs::home_dir().as_ref() == Some(path),
            LocalOrRemotePath::Remote(remote_path) => self
                .remote_home_directories
                .get(&remote_path.host_id)
                .is_some_and(|home_dir| home_dir == path),
        }
    }

    fn skill_by_location(&self, location: &LocalOrRemotePath) -> Option<&ParsedSkill> {
        self.skills_by_path.get(location).or_else(|| {
            location
                .as_remote()
                .and_then(|remote| self.bundled_skills.remote_skill_by_path(remote))
        })
    }

    /// Removes every skill registered under `directory`'s exact key (not descendants).
    /// Used to tear down one remote host's home skills wholesale on disconnect/refresh.
    fn remove_skills_for_directory(&mut self, directory: &LocalOrRemotePath) {
        let Some(skill_paths) = self.directory_skills.remove(directory) else {
            return;
        };
        for skill_path in skill_paths {
            self.remove_skill_by_path(&skill_path);
        }
    }

    fn remove_skill_by_path(&mut self, skill_path: &LocalOrRemotePath) {
        let Some(skill) = self.skills_by_path.remove(skill_path) else {
            return;
        };
        let remove_name = self
            .skills_by_name
            .get_mut(&skill.name)
            .is_some_and(|paths| {
                paths.remove(skill_path);
                paths.is_empty()
            });
        if remove_name {
            self.skills_by_name.remove(&skill.name);
        }
    }

    fn handle_skill_watcher_event(
        &mut self,
        event: SkillWatcherEvent,
        ctx: &mut ModelContext<Self>,
    ) {
        match event {
            SkillWatcherEvent::SkillsAdded { skills } => {
                self.handle_skills_added(skills, ctx);
            }
            SkillWatcherEvent::SkillsDeleted { paths } => {
                self.handle_skills_deleted(paths, ctx);
            }
        }
    }

    pub fn handle_skills_added(&mut self, skills: Vec<ParsedSkill>, ctx: &mut ModelContext<Self>) {
        if skills.is_empty() {
            return;
        }

        for skill in skills {
            match extract_skill_parent_directory(&skill.path) {
                Ok(parent_dir) => {
                    self.directory_skills
                        .entry(parent_dir)
                        .or_default()
                        .insert(skill.path.clone());

                    self.skills_by_name
                        .entry(skill.name.clone())
                        .or_default()
                        .insert(skill.path.clone());
                    self.skills_by_path.insert(skill.path.clone(), skill);
                }
                Err(_) => {
                    log::warn!(
                        "Could not extract parent directory for skill: {:?}",
                        skill.path
                    );
                }
            }
        }

        ctx.emit(SkillManagerEvent::InventoryChanged);
    }

    fn handle_skills_deleted(&mut self, paths: Vec<PathBuf>, ctx: &mut ModelContext<Self>) {
        if paths.is_empty() {
            return;
        }

        for path in paths {
            self.handle_path_deleted(&LocalOrRemotePath::Local(path));
        }

        ctx.emit(SkillManagerEvent::InventoryChanged);
    }

    fn handle_path_deleted(&mut self, path: &LocalOrRemotePath) {
        // Delete all skills that are affected by this deleted path
        for (dir, skill_paths) in &self.directory_skills.clone() {
            if dir.starts_with(path) {
                // Delete this entire entry and remove all skill_paths under this directory from cache
                for skill_path in skill_paths {
                    self.remove_skill_by_path(skill_path);
                }
                self.directory_skills.remove(dir);
            } else if path.starts_with(dir) {
                // Remove all skills under this directory that is a child of the deleted path
                for skill_path in skill_paths {
                    if skill_path.starts_with(path) {
                        self.remove_skill_by_path(skill_path);
                        self.directory_skills
                            .entry(dir.clone())
                            .or_default()
                            .remove(skill_path);
                    }
                }
            }
        }
    }

    /// Adds a skill to the skill manager for testing purposes.
    #[cfg(test)]
    pub fn add_skill_for_testing(&mut self, skill: ParsedSkill) {
        let path = skill.path.clone();
        let name = skill.name.clone();
        self.skills_by_path.insert(path.clone(), skill);
        self.skills_by_name.entry(name).or_default().insert(path);
    }

    /// Adds a bundled skill to the local catalog, for testing purposes.
    #[cfg(test)]
    pub fn add_bundled_skill_for_testing(
        &mut self,
        id: impl Into<String>,
        skill: ParsedSkill,
        activation: BundledSkillActivation,
    ) {
        self.bundled_skills.insert_local_for_testing(id, skill, activation);
    }

    /// Adds a bundled skill to a remote host's catalog, for testing purposes.
    #[cfg(test)]
    pub fn add_remote_bundled_skill_for_testing(
        &mut self,
        host_id: HostId,
        id: impl Into<String>,
        skill: ParsedSkill,
        activation: BundledSkillActivation,
    ) {
        self.bundled_skills
            .insert_remote_for_testing(host_id, id, skill, activation);
    }
}

impl Entity for SkillManager {
    type Event = SkillManagerEvent;
}

impl SingletonEntity for SkillManager {}

fn path_matches_reference_location(path: &LocalOrRemotePath, reference: &SkillReference) -> bool {
    match (path, reference) {
        (
            LocalOrRemotePath::Remote(path),
            SkillReference::Path(LocalOrRemotePath::Remote(reference)),
        ) => path.host_id == reference.host_id,
        (
            LocalOrRemotePath::Local(_),
            SkillReference::Path(LocalOrRemotePath::Local(_)) | SkillReference::BundledSkillId(_),
        ) => true,
        (LocalOrRemotePath::Local(_), SkillReference::Path(LocalOrRemotePath::Remote(_)))
        | (
            LocalOrRemotePath::Remote(_),
            SkillReference::Path(LocalOrRemotePath::Local(_)) | SkillReference::BundledSkillId(_),
        ) => false,
    }
}

#[cfg(test)]
#[path = "skill_manager_tests.rs"]
mod tests;
