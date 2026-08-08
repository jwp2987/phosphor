#[path = "file_watchers/mod.rs"]
mod file_watchers;
pub use file_watchers::{extract_skill_parent_directory, SkillWatcher, SkillWatcherEvent};

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use super::bundled::BundledSkill;
pub use super::bundled::BundledSkillActivation;
#[cfg(test)]
use super::bundled::{build_bundled_skill_context, read_bundled_skills};
use super::SkillDescriptor;
use crate::ai::skills::skill_utils::unique_skills;
use ai::skills::{get_provider_for_path, provider_rank, ParsedSkill, SkillProvider, SkillReference};
use warp_core::features::FeatureFlag;
use warp_util::local_or_remote_path::LocalOrRemotePath;
use warpui::{AppContext, Entity, ModelContext, ModelHandle, SingletonEntity};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillManagerEvent {
    InventoryChanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillInventoryDuplicate {
    pub path: PathBuf,
    pub name: String,
    pub description: String,
    pub content: String,
    pub provider: SkillProvider,
    pub scope: ai::skills::SkillScope,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillInventoryItem {
    pub name: String,
    pub default_skill: SkillInventoryDuplicate,
    pub duplicates: Vec<SkillInventoryDuplicate>,
}

impl SkillInventoryItem {
    pub fn has_duplicates(&self) -> bool {
        self.duplicates.len() > 1
    }
}

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
    directory_skills: HashMap<PathBuf, HashSet<PathBuf>>,
    skills_by_path: HashMap<PathBuf, ParsedSkill>,
    /// Reverse lookup: skill name → set of paths with that name.
    /// This allows efficient lookup by skill name without scanning all paths.
    skills_by_name: HashMap<String, HashSet<PathBuf>>,
    /// Skills bundled into Zap.
    bundled: BundledSkill,
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
                me.bundled = result;
            });
        }

        Self {
            directory_skills: HashMap::new(),
            skills_by_path: HashMap::new(),
            skills_by_name: HashMap::new(),
            bundled: BundledSkill::default(),
            skill_watcher,
        }
    }

    /// Returns skills available for the given working directory.
    pub fn get_skills_for_working_directory(
        &self,
        working_directory: Option<&Path>,
        ctx: &AppContext,
    ) -> Vec<SkillDescriptor> {
        // Collect skill paths as (dir_path, skill_path) tuples for later deduplication.
        // Home skills use the home directory as their dir_path; project skills use their
        // owning directory.
        let mut skill_paths = Vec::new();

        if let Some(home_dir) = dirs::home_dir() {
            skill_paths.extend(
                self.home_skill_paths()
                    .into_iter()
                    .map(|path| (home_dir.clone(), path)),
            );
        }

        if let Some(working_directory) = working_directory {
            let repo_root = repo_metadata::repositories::DetectedRepositories::as_ref(ctx)
                .get_root_for_path(working_directory);

            for (dir, dir_skill_paths) in &self.directory_skills {
                if is_home_directory(dir) {
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
        let mut skills = unique_skills(&skill_paths, &self.skills_by_path);

        // Apply icon overrides for well-known skill names (e.g. partner integrations).
        for skill in &mut skills {
            if skill.icon_override.is_none() {
                skill.icon_override =
                    crate::ai::skills::skill_utils::icon_override_for_skill_name(&skill.name);
            }
        }

        // Append bundled skills whose activation condition is met.
        if FeatureFlag::BundledSkills.is_enabled() {
            skills.extend(self.bundled.active_descriptors(ctx));
        }

        skills
    }

    /// Returns the currently-known home skill file paths.
    pub fn home_skill_paths(&self) -> Vec<PathBuf> {
        let Some(home_dir) = dirs::home_dir() else {
            return vec![];
        };
        self.directory_skills
            .get(&home_dir)
            .map(|skills| skills.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Returns the currently-known directories which have skills registered.
    /// This includes both repo roots and subdirectories with skills.
    pub fn directories_with_skills(&self) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = self.directory_skills.keys().cloned().collect();
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

        for (dir, skill_paths) in &self.directory_skills {
            // Include skills from directories that are under scope_dir
            if dir.starts_with(scope_dir) {
                paths.extend(skill_paths.iter().cloned());
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
        self.skill_paths_by_name(&skill.name)
            .iter()
            .filter_map(|path| get_provider_for_path(&LocalOrRemotePath::Local(path.clone())))
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
        self.skill_paths_by_name(&skill.name)
            .iter()
            .filter_map(|path| get_provider_for_path(&LocalOrRemotePath::Local(path.clone())))
            .filter(|provider| supported_providers.contains(provider))
            .min_by_key(|provider| provider_rank(*provider))
            .unwrap_or(skill.provider)
    }

    /// Returns skill file paths that have the given skill name.
    /// A skill's name comes from the `name` field in its SKILL.md front matter.
    pub fn skill_paths_by_name(&self, name: &str) -> Vec<PathBuf> {
        self.skills_by_name
            .get(name)
            .map(|paths| {
                let mut paths: Vec<PathBuf> = paths.iter().cloned().collect();
                paths.sort();
                paths
            })
            .unwrap_or_default()
    }

    /// Returns a reference to a parsed skill for a specific SKILL.md file path, if it is cached.
    pub fn skill_by_path(&self, skill_path: &Path) -> Option<&ParsedSkill> {
        self.skills_by_path.get(skill_path)
    }

    /// Returns the appropriate `SkillReference` for a skill at the given path.
    /// For bundled skills, returns `BundledSkillId`; otherwise returns `Path`.
    pub fn reference_for_skill_path(&self, skill_path: &Path) -> SkillReference {
        self.bundled
            .reference_for_path(skill_path)
            .unwrap_or_else(|| SkillReference::Path(skill_path.to_path_buf()))
    }

    /// Get the definition of a skill, if it is cached.
    pub fn skill_by_reference(&self, reference: &SkillReference) -> Option<&ParsedSkill> {
        match reference {
            SkillReference::Path(path) => self.skills_by_path.get(path),
            SkillReference::BundledSkillId(id) => self.bundled.skill(id),
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
    /// with `unique_skills`/`list_skill_inventory`. Bundled skills don't enter the
    /// `skills_by_name` index, so they're iterated separately here as a fallback.
    pub fn find_skill_by_name(&self, name: &str) -> Option<&ParsedSkill> {
        // Prefers filesystem skills: when multiple copies share a name, picks the best by provider_rank.
        let best_fs_path = self
            .skill_paths_by_name(name)
            .into_iter()
            .min_by_key(|path| {
                get_provider_for_path(&LocalOrRemotePath::Local(path.clone()))
                    .map(provider_rank)
                    .unwrap_or(usize::MAX)
            });
        if let Some(path) = best_fs_path {
            if let Some(skill) = self.skills_by_path.get(&path) {
                return Some(skill);
            }
        }
        // Fallback: bundled skills (matched by name rather than id).
        self.bundled
            .iter()
            .map(|(_, skill)| skill)
            .find(|skill| skill.name == name)
    }

    /// Returns a bundled skill by ID only if its activation condition is met.
    pub fn active_bundled_skill(&self, id: &str, ctx: &AppContext) -> Option<&ParsedSkill> {
        self.bundled.active_skill(id, ctx)
    }

    /// Get the definition of a skill only if it is currently available for invocation.
    ///
    /// Path-based user skills are always controlled by normal path scoping. Bundled
    /// skills additionally respect their runtime activation state, so a stale
    /// `BundledSkillId` reference (e.g. copied from an earlier response, or for a
    /// skill whose activation condition has since flipped off) cannot invoke a
    /// disabled bundled skill.
    ///
    /// Ported from the pin's `SkillManager::active_skill_by_reference` (`02b53fcd8`).
    /// The pin also has `active_skill_by_reference_with_origin`, which additionally
    /// dispatches to a *remote* host's bundled-skill catalog for a `WarpifiedRemote`
    /// session (keyed by `SkillPathOrigin`/`HostId`, over `LocalOrRemotePath`). That
    /// half is deliberately not ported here: `SkillReference::Path` and
    /// `ParsedSkill::path` in this fork are still plain `PathBuf` (issue #299 tracks
    /// migrating them to `LocalOrRemotePath`), and there is no per-host bundled-skill
    /// catalog to dispatch to (issue #487/#493 explicitly scoped that out too, for
    /// the same missing-prerequisite reason). This method only ever resolves against
    /// the local catalog — equivalent to always passing the pin's
    /// `SkillPathOrigin::Local`. See issue #370.
    pub fn active_skill_by_reference(
        &self,
        reference: &SkillReference,
        ctx: &AppContext,
    ) -> Option<&ParsedSkill> {
        match reference {
            SkillReference::Path(path) => self.skills_by_path.get(path),
            SkillReference::BundledSkillId(id) => self.active_bundled_skill(id, ctx),
        }
    }

    pub fn list_skill_inventory(&self, ctx: &AppContext) -> Vec<SkillInventoryItem> {
        let _ = ctx;
        let mut by_name: HashMap<String, Vec<SkillInventoryDuplicate>> = HashMap::new();

        for skill in self.skills_by_path.values() {
            by_name
                .entry(skill.name.clone())
                .or_default()
                .push(SkillInventoryDuplicate {
                    path: skill.path.clone(),
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
            match extract_skill_parent_directory(&skill.path) { Ok(parent_dir) => {
                self.directory_skills
                    .entry(parent_dir)
                    .or_default()
                    .insert(skill.path.clone());

                self.skills_by_name
                    .entry(skill.name.clone())
                    .or_default()
                    .insert(skill.path.clone());
                self.skills_by_path.insert(skill.path.clone(), skill);
            } _ => {
                log::warn!(
                    "Could not extract parent directory for skill: {:?}",
                    skill.path
                );
            }}
        }

        ctx.emit(SkillManagerEvent::InventoryChanged);
    }

    fn handle_skills_deleted(&mut self, paths: Vec<PathBuf>, ctx: &mut ModelContext<Self>) {
        if paths.is_empty() {
            return;
        }

        for path in paths {
            self.handle_path_deleted(&path);
        }

        ctx.emit(SkillManagerEvent::InventoryChanged);
    }

    fn handle_path_deleted(&mut self, path: &Path) {
        // Delete all skills that are affected by this deleted path
        for (dir, skill_paths) in &self.directory_skills.clone() {
            if dir.starts_with(path) {
                // Delete this entire entry and remove all skill_paths under this directory from cache
                for skill_path in skill_paths {
                    let skill = self.skills_by_path.remove(skill_path);
                    if let Some(skill) = skill {
                        self.skills_by_name
                            .entry(skill.name.clone())
                            .or_default()
                            .remove(skill_path);
                    }
                }
                self.directory_skills.remove(dir);
            } else if path.starts_with(dir) {
                // Remove all skills under this directory that is a child of the deleted path
                for skill_path in skill_paths {
                    if skill_path.starts_with(path) {
                        let skill = self.skills_by_path.remove(skill_path);
                        if let Some(skill) = skill {
                            self.skills_by_name
                                .entry(skill.name.clone())
                                .or_default()
                                .remove(skill_path);
                        }
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

    /// Adds a bundled skill to the skill manager for testing purposes.
    ///
    /// Ported from the pin's `SkillManager::add_bundled_skill_for_testing`
    /// (`02b53fcd8`), local-catalog-only (see [`Self::active_skill_by_reference`]).
    /// The pin's remote-host counterpart, `add_remote_bundled_skill_for_testing`,
    /// is not ported for the same reason. Forwards to
    /// [`BundledSkill::insert_for_testing`], which now owns the catalog storage
    /// (extracted from this struct by #493).
    #[cfg(test)]
    pub fn add_bundled_skill_for_testing(
        &mut self,
        id: impl Into<String>,
        skill: ParsedSkill,
        activation: BundledSkillActivation,
    ) {
        self.bundled.insert_for_testing(id, skill, activation);
    }
}

fn is_home_directory(path: &Path) -> bool {
    let Some(home_dir) = dirs::home_dir() else {
        return false;
    };
    path == home_dir
}

impl Entity for SkillManager {
    type Event = SkillManagerEvent;
}

impl SingletonEntity for SkillManager {}

#[cfg(test)]
#[path = "skill_manager_tests.rs"]
mod tests;
