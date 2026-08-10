use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use ai::skills::{
    ParsedSkill, SKILL_PROVIDER_DEFINITIONS, SkillProvider, home_skills_path, parse_skill,
    read_skills,
};
use anyhow::Error;
use regex::Regex;
use repo_metadata::{
    RepoContent, RepoMetadataModel, RepositoryIdentifier, local_model::GetContentsArgs,
};
use walkdir::{DirEntry, WalkDir};
use warp_util::local_or_remote_path::LocalOrRemotePath;
use warp_util::remote_path::RemotePath;
use warp_util::standardized_path::StandardizedPath;
use warpui::AppContext;

use crate::warp_managed_paths_watcher::warp_managed_skill_dirs;

fn local_or_remote_path_for_repo_path(
    repo_id: &RepositoryIdentifier,
    path: &StandardizedPath,
) -> LocalOrRemotePath {
    match repo_id {
        RepositoryIdentifier::Local(_) => LocalOrRemotePath::Local(path.to_local_path_lossy()),
        RepositoryIdentifier::Remote(remote) => {
            // `RepositoryIdentifier::Remote` carries a `warp_core::HostId`, but
            // `warp_util::remote_path::RemotePath` takes a `warp_util::host_id::HostId` --
            // distinct types in this fork though unified in the pin. Bridge rather than clone.
            LocalOrRemotePath::Remote(RemotePath::new(
                crate::code::buffer_location::core_host_id_to_util(&remote.host_id),
                path.clone(),
            ))
        }
    }
}

/// Finds project skill files from stored standing results.
///
/// Symlinked project skills are resolved while evaluating standing queries on the process that
/// owns the repository. This consumer treats those results as authoritative for both local and
/// remote repositories; direct filesystem discovery remains confined to metadata-failure fallback.
pub(super) fn find_project_skill_files_in_tree(
    repo_id: &RepositoryIdentifier,
    repo_metadata: &RepoMetadataModel,
    ctx: &AppContext,
) -> Vec<LocalOrRemotePath> {
    repo_metadata
        .standing_query_results(repo_id, ctx)
        .into_iter()
        .flat_map(|results| results.project_skills())
        .filter(|content| !content.is_directory)
        .map(|content| local_or_remote_path_for_repo_path(repo_id, &content.path))
        .collect()
}

/// Finds local project skill files by discovering provider directories on the filesystem.
///
/// This is a local-only fallback for repositories whose repo metadata indexing fails. Successful
/// local and remote project refreshes should use [`find_project_skill_files_in_tree`] so the
/// normal metadata-backed path remains shared.
pub(super) fn find_local_project_skill_files_on_filesystem(
    scan_root: &Path,
) -> Vec<LocalOrRemotePath> {
    let direct_skill_file = scan_root.join("SKILL.md");
    if is_skill_file(&direct_skill_file) {
        return vec![LocalOrRemotePath::Local(direct_skill_file)];
    }

    find_local_provider_directories_on_filesystem(scan_root)
        .into_iter()
        .flat_map(|provider_dir| std::fs::read_dir(provider_dir).into_iter().flatten())
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let skill_dir = entry.path();
            if !skill_dir.is_dir() {
                return None;
            }
            let skill_file = skill_dir.join("SKILL.md");
            skill_file
                .exists()
                .then_some(LocalOrRemotePath::Local(skill_file))
        })
        .collect()
}

fn find_local_provider_directories_on_filesystem(scan_root: &Path) -> Vec<PathBuf> {
    let mut provider_dirs = Vec::new();
    let mut entries = WalkDir::new(scan_root).follow_links(false).into_iter();
    while let Some(entry) = entries.next() {
        let Ok(entry) = entry else {
            continue;
        };
        if is_ignored_fallback_scan_entry(&entry) {
            if entry.file_type().is_dir() {
                entries.skip_current_dir();
            }
            continue;
        }
        if entry.file_type().is_dir() && is_project_provider_path(entry.path()) {
            provider_dirs.push(entry.into_path());
            entries.skip_current_dir();
        }
    }
    provider_dirs.sort();
    provider_dirs
}

fn is_ignored_fallback_scan_entry(entry: &DirEntry) -> bool {
    entry.file_name().to_str() == Some(".git")
}

fn is_project_provider_path(path: &Path) -> bool {
    SKILL_PROVIDER_DEFINITIONS
        .iter()
        .any(|provider| path.ends_with(&provider.skills_path))
}

/// Finds all skill directories in a repository by querying the RepoMetadataModel tree.
///
/// Returns a list of paths to skill directories (e.g., `/repo/.agents/skills/`, `/repo/sub/.claude/skills/`).
pub fn find_skill_directories_in_tree(
    repo_path: &Path,
    repo_metadata: &RepoMetadataModel,
    ctx: &AppContext,
) -> Vec<PathBuf> {
    // Collect provider skills paths (e.g., ".agents/skills", ".claude/skills")
    let skill_path_suffixes: Vec<&Path> = SKILL_PROVIDER_DEFINITIONS
        .iter()
        .map(|p| p.skills_path.as_path())
        .collect();

    // Filter during traversal: only collect directories that end with a skill provider path.
    // The filter rejects files and non-matching directories, avoiding intermediate allocations.
    let args = GetContentsArgs::default().with_filter(move |content| {
        let RepoContent::Directory(dir) = content else {
            return false;
        };
        skill_path_suffixes
            .iter()
            .any(|suffix| dir.path.ends_with(&suffix.to_string_lossy()))
    });

    let Some(id) = repo_metadata::RepositoryIdentifier::try_local(repo_path) else {
        return Vec::new();
    };
    repo_metadata
        .get_repo_contents(&id, args, ctx)
        .unwrap_or_default()
        .contents
        .into_iter()
        // Only directories should reach this iterator due to the GetContentsArgs::filter.
        // Keep the File arm for exhaustive matching in case RepoContent grows new variants.
        .map(|content| match content {
            RepoContent::Directory(dir) => dir.path.to_local_path_lossy(),
            RepoContent::File(f) => f.path.to_local_path_lossy(),
        })
        .collect()
}

/// Reads all skills from the given skill directories.
pub fn read_skills_from_directories(
    skill_dirs: impl IntoIterator<Item = PathBuf>,
) -> Vec<ParsedSkill> {
    skill_dirs
        .into_iter()
        .flat_map(|dir| read_skills(&dir))
        .collect()
}

/// Reads all skills from the given concrete skill files.
pub fn read_skills_from_files(skill_files: impl IntoIterator<Item = PathBuf>) -> Vec<ParsedSkill> {
    skill_files
        .into_iter()
        .filter_map(|path| parse_skill(&path).ok())
        .collect()
}

pub fn is_skill_file(path: &Path) -> bool {
    extract_skill_parent_directory(&LocalOrRemotePath::Local(path.to_path_buf())).is_ok()
}

static SKILL_PROVIDER_PATHS: LazyLock<HashSet<String>> = LazyLock::new(|| {
    // Collect the skill provider paths from the definitions
    SKILL_PROVIDER_DEFINITIONS
        .iter()
        .map(|p| p.skills_path.to_string_lossy().to_string())
        .collect()
});

// Pattern: {prefix}/{provider_path}/{skill-name}/SKILL.md
// where provider_path is 2 parts (e.g., ".agents/skills") and skill-name is 1 part
#[cfg(not(target_os = "windows"))]
static SKILL_FILE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(.+)/([^/]+/[^/]+)/[^/]+/SKILL\.md$")
        .expect("Failed to compile skill file pattern")
});

// On windows, the path separator is \
#[cfg(target_os = "windows")]
static SKILL_FILE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(.+)\\([^\\]+\\[^\\]+)\\[^\\]+\\SKILL\.md$")
        .expect("Failed to compile skill file pattern")
});

/// Finds the directory that owns a skill's provider folder (e.g. the repo root that owns
/// `.agents/skills/`), given the skill's `SKILL.md` path.
///
/// Ported to `LocalOrRemotePath` for #299/#487: a remote host's home skills (published by
/// its daemon and reconciled client-side in `remote_agent_context.rs`) flow through
/// [`crate::ai::skills::SkillManager::handle_skills_added`] the same way local
/// watcher-discovered skills do, so this must accept a remote path without an artificial
/// local-only guard. Local paths keep the exact original regex-based matching (unchanged,
/// already covered by `utils_tests.rs`); remote paths use structural parent-walking
/// instead, since [`SKILL_FILE_PATTERN`] is platform-separator-specific and a remote
/// path's separator does not depend on the local OS.
pub fn extract_skill_parent_directory(
    path: &LocalOrRemotePath,
) -> Result<LocalOrRemotePath, Error> {
    match path {
        LocalOrRemotePath::Local(local_path) => {
            extract_local_skill_parent_directory(local_path).map(LocalOrRemotePath::Local)
        }
        LocalOrRemotePath::Remote(_) => extract_remote_skill_parent_directory(path),
    }
}

fn extract_local_skill_parent_directory(path: &Path) -> Result<PathBuf, Error> {
    let is_warp_home_skill = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "SKILL.md")
        && path
            .parent()
            .and_then(Path::parent)
            .is_some_and(|parent| warp_managed_skill_dirs().iter().any(|dir| parent == dir));
    if is_warp_home_skill {
        return dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Home directory not available for {}", path.display()));
    }
    let path_str = path.to_string_lossy();

    if let Some(captures) = SKILL_FILE_PATTERN.captures(&path_str) {
        if let Some(provider_path) = captures.get(2) {
            if SKILL_PROVIDER_PATHS.contains(provider_path.as_str()) {
                if let Some(parent_directory) = captures.get(1) {
                    return Ok(PathBuf::from(parent_directory.as_str()));
                }
            }
        }
    }

    Err(anyhow::anyhow!("Not a skill path: {}", path.display()))
}

fn extract_remote_skill_parent_directory(
    path: &LocalOrRemotePath,
) -> Result<LocalOrRemotePath, Error> {
    if path.file_name() != Some("SKILL.md") {
        return Err(anyhow::anyhow!("Not a skill path: {}", path.display_path()));
    }
    let Some(skill_dir) = path.parent() else {
        return Err(anyhow::anyhow!("Not a skill path: {}", path.display_path()));
    };
    let Some(skills_root) = skill_dir.parent() else {
        return Err(anyhow::anyhow!("Not a skill path: {}", path.display_path()));
    };

    ai::skills::provider_parent_directory_for_skills_root(&skills_root)
        .ok_or_else(|| anyhow::anyhow!("Not a skill path: {}", path.display_path()))
}

/// Check if this path is a skill directory under a home directory provider path
/// E.g. ~/.agents/skills/skill-name
pub fn is_home_skill_directory(path: &Path) -> bool {
    let parent_directory = path.parent();
    if let Some(parent_directory) = parent_directory {
        is_home_provider_path(parent_directory)
    } else {
        false
    }
}

/// Check if this path is a home directory provider path
/// E.g. ~/.agents/skills
pub fn is_home_provider_path(path: &Path) -> bool {
    SKILL_PROVIDER_DEFINITIONS.iter().any(|provider| {
        if provider.provider == SkillProvider::Zap {
            return warp_managed_skill_dirs().iter().any(|dir| path == dir);
        }
        home_skills_path(provider.provider)
            .as_ref()
            .is_some_and(|home_skills_path| path == home_skills_path)
    })
}

#[cfg(test)]
#[path = "utils_tests.rs"]
mod tests;
