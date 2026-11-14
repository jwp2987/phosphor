//! The file tree over the shell backend (Wave H parity, P3, family `b`): `NavigatedToDirectory`
//! and `LoadRepoMetadataDirectory` served by ordinary commands over the out-of-band executor.
//!
//! The commands and parsers are [`crate::shellcmd::tree`], shared with the helper. A git
//! repository is snapshotted with two `git ls-files` calls (non-ignored files; ignored paths with
//! directories collapsed), a directory outside git with one level of listing. Directories are
//! listed lazily afterwards, exactly like the helper.
//!
//! There is no connection to push on and no watcher: the snapshot (and, later, the differences a
//! [`refresh_repo_tree`] finds) are queued as [`ShellPush`]es for the caller to drain, and a
//! refresh only happens when the caller asks, which the app does on prompt and cwd events
//! (`RefreshCoordinator`) and never on a timer.
//!
//! Known differences from the helper: empty directories do not appear in a git repository's
//! snapshot (`git ls-files` does not list them; they appear when their parent is listed), and a
//! symlink to a directory is only omitted where `find -printf` exists (the `ls` fallback on
//! BSD/macOS/busybox lists it as a file).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Mutex;

use super::ShellBackend;
use super::git_batch::join_key;
use crate::client::ClientError;
use crate::proto::{
    DirEntry, ErrorCode, FileOperationError, FileSystemEntryKind, ListDirectoryResponse,
    ListDirectorySuccess, LoadRepoMetadataDirectoryResponse, NavigatedToDirectoryResponse,
    RepoMetadataSnapshot, RepoMetadataUpdatePush, list_directory_response,
};
use crate::shellcmd::agent_context;
use crate::shellcmd::tree::{
    MAX_IGNORED_LIST_BYTES, MAX_REPO_LIST_BYTES, TreeChild, apply_refresh, canonical_dir_command,
    dir_is_ignored, dir_update, finish_children, git_root_command, ignored_children_command,
    is_within, list_dir_command, list_dir_stat_command, parse_list_records, parse_nav_target,
    parse_path_list, parse_stat_records, repo_files_command, repo_ignored_command,
    snapshot_entries, tree_from_paths,
};
use crate::shellcmd::{Frame, ParsedSection, Script, StderrMode};

use super::super::BackendError;

/// What the shell backend queues for the caller to emit, the counterparts of the helper's
/// unsolicited `RepoMetadataSnapshot` and `RepoMetadataUpdatePush` messages.
#[derive(Clone, Debug)]
pub enum ShellPush {
    RepoMetadataSnapshot(RepoMetadataSnapshot),
    RepoMetadataUpdate(RepoMetadataUpdatePush),
    /// The host's skills and global rules changed (or were read for the first time).
    AgentContext(crate::proto::RemoteAgentContextSnapshot),
}

struct CachedRepo {
    is_git: bool,
    /// Project skill files as last reported (standing-query results).
    skill_files: BTreeSet<String>,
    dirs: BTreeMap<String, Vec<TreeChild>>,
}

/// The trees one host's shell backend has reported, by root path.
#[derive(Default)]
pub struct TreeCache {
    repos: Mutex<HashMap<String, CachedRepo>>,
    /// The repository root each navigated directory resolved to (git's own answer at that
    /// navigation), by the directory's [`join_key`]. Only directories inside a repository.
    navigated: Mutex<HashMap<String, String>>,
}

impl TreeCache {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, CachedRepo>> {
        self.repos.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Every indexed root (code review and link validation register these).
    pub fn roots(&self) -> Vec<String> {
        self.lock().keys().cloned().collect()
    }

    /// Forgets every tree, e.g. when the host disconnects.
    pub fn clear(&self) {
        self.lock().clear();
        self.navigated
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }

    /// The repository root the navigation of `dir` resolved (`None`: never navigated, or it was
    /// outside git). The side calls of a prompt in `dir` are batched under that root (#2216).
    pub(super) fn navigated_root(&self, dir: &str) -> Option<String> {
        self.navigated
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&join_key(dir))
            .cloned()
    }

    fn remember_navigation(&self, dir: &str, git_root: Option<&str>) {
        let mut navigated = self.navigated.lock().unwrap_or_else(|e| e.into_inner());
        match git_root {
            Some(root) => navigated.insert(join_key(dir), root.to_owned()),
            None => navigated.remove(&join_key(dir)),
        };
    }
}

fn server_error(code: ErrorCode, message: String) -> BackendError {
    BackendError::Client(ClientError::ServerError { code, message })
}

fn new_script() -> Result<Script, BackendError> {
    Ok(Script::new(ShellBackend::new_nonce())?)
}

/// Lists one directory with the two sections of its listing script.
fn listing_script(
    script: Script,
    kind: crate::shellcmd::ShellKind,
    dir: &str,
    with_ignored: bool,
    suffix: &str,
) -> Result<Script, BackendError> {
    let script = script.section(
        format!("list{suffix}"),
        list_dir_command(kind, dir),
        StderrMode::Discard,
    )?;
    if with_ignored {
        Ok(script.section(
            format!("ignored{suffix}"),
            ignored_children_command(kind, dir),
            StderrMode::Discard,
        )?)
    } else {
        Ok(script)
    }
}

fn children_from_frame(
    frame: &crate::shellcmd::Frame,
    suffix: &str,
    in_git: bool,
    all_ignored: bool,
) -> Result<Vec<TreeChild>, String> {
    let list = frame
        .section(&format!("list{suffix}"))
        .ok_or_else(|| "no listing from the host".to_owned())?;
    if !list.success() {
        return Err("the directory cannot be listed".to_owned());
    }
    let ignored: BTreeSet<String> = frame
        .section(&format!("ignored{suffix}"))
        .map(|s| parse_path_list(&s.output, false).into_iter().collect())
        .unwrap_or_default();
    let mut children = finish_children(parse_list_records(&list.output), &ignored, in_git);
    if all_ignored {
        for c in &mut children {
            c.ignored = true;
        }
    }
    Ok(children)
}

/// A repository's snapshot listings. A repository too big for one call is served lazily from
/// the root only.
///
/// `listed` holds the lists already read for `root` (the navigation's batch); otherwise the
/// refresh's batched script is asked (#2156). A list that is missing from what was read, or was
/// cut by the batch's smaller cap, is read here by its own section, both missing ones in ONE exec.
async fn git_snapshot(
    shell: &ShellBackend,
    root: &str,
    listed: Option<&Frame>,
) -> Result<BTreeMap<String, Vec<TreeChild>>, BackendError> {
    let kind = shell.shell_kind();
    let fetched = if listed.is_none() {
        shell.batched_tree(root, "refresh_tree").await
    } else {
        None
    };
    let listed = listed.or_else(|| fetched.as_ref().and_then(|b| b.tree()));
    let have =
        |name: &str| -> Option<ParsedSection> { listed.and_then(|f| f.section(name)).cloned() };
    let mut files = have("files");
    let mut ignored = have("ignoredpaths");
    if files.is_none() || ignored.is_none() {
        let mut script = new_script()?;
        if files.is_none() {
            script =
                script.section("files", repo_files_command(kind, root), StderrMode::Discard)?;
        }
        if ignored.is_none() {
            script = script.section(
                "ignoredpaths",
                repo_ignored_command(kind, root),
                StderrMode::Discard,
            )?;
        }
        let frame = shell.run_script(&script).await?;
        if files.is_none() {
            files = frame.section("files").cloned();
        }
        if ignored.is_none() {
            ignored = frame.section("ignoredpaths").cloned();
        }
    }
    let files = files.filter(|s| s.success());
    let too_big = files
        .as_ref()
        .is_some_and(|s| s.output.len() >= MAX_REPO_LIST_BYTES);
    let Some(files) = files.filter(|_| !too_big) else {
        return lazy_root(shell, root, true).await;
    };
    let files = parse_path_list(&files.output, false);
    let mut ignored = ignored
        .filter(|s| s.success())
        .map(|s| parse_path_list(&s.output, s.output.len() >= MAX_IGNORED_LIST_BYTES))
        .unwrap_or_default();
    // `git ls-files` never lists the repository's own metadata directory.
    ignored.push(".git/".to_owned());
    Ok(tree_from_paths(root, &files, &ignored))
}

/// Just the root's children.
async fn lazy_root(
    shell: &ShellBackend,
    root: &str,
    in_git: bool,
) -> Result<BTreeMap<String, Vec<TreeChild>>, BackendError> {
    let script = listing_script(new_script()?, shell.shell_kind(), root, in_git, "")?;
    let frame = shell.run_script(&script).await?;
    let children = children_from_frame(&frame, "", in_git, false).map_err(|m| {
        server_error(
            ErrorCode::Internal,
            format!("Failed to load directory: {m}"),
        )
    })?;
    Ok(BTreeMap::from([(root.to_owned(), children)]))
}

pub(super) async fn navigate_to_directory(
    shell: &ShellBackend,
    path: String,
) -> Result<NavigatedToDirectoryResponse, BackendError> {
    let kind = shell.shell_kind();
    // The probe and, in a repository, its lists come from the refresh's batched script when there
    // is one (#2156): a `cd` into a repository is then one exec for all the side work.
    let batch = shell.batched_tree(&path, "navigate_tree").await;
    let pre = batch
        .as_ref()
        .and_then(|b| b.tree())
        .filter(|f| f.section("canon").is_some() && f.section("git").is_some());
    let own: Frame;
    let frame: &Frame = match pre {
        Some(frame) => frame,
        None => {
            let script = new_script()?
                .section(
                    "canon",
                    canonical_dir_command(kind, &path),
                    StderrMode::Discard,
                )?
                .section("git", git_root_command(kind, &path), StderrMode::Discard)?;
            own = shell.run_script(&script).await?;
            &own
        }
    };
    let target = parse_nav_target(frame)
        .map_err(|m| server_error(ErrorCode::InvalidRequest, format!("Invalid path: {m}")))?;
    let (root, is_git) = match target.git_root {
        Some(git) if is_within(&git, &target.dir) => (git, true),
        _ => (target.dir.clone(), false),
    };
    let dirs = if is_git {
        // The batch listed `path` itself: its lists are the root's only when `path` is the root.
        let listed = pre.filter(|_| root == target.dir);
        git_snapshot(shell, &root, listed).await?
    } else {
        lazy_root(shell, &root, false).await?
    };
    // Both spellings of the directory (as asked and canonical) lead to its repository's batch.
    for dir in [path.as_str(), target.dir.as_str()] {
        shell
            .tree_cache
            .remember_navigation(dir, is_git.then_some(root.as_str()));
    }
    let skill_files = agent_context::project_skill_files(&dirs);
    shell.push(ShellPush::RepoMetadataSnapshot(RepoMetadataSnapshot {
        repo_path: root.clone(),
        entries: snapshot_entries(&root, &dirs),
        sync_complete: true,
        standing_results: agent_context::standing_snapshot(&skill_files),
    }));
    shell.tree_cache.lock().insert(
        root.clone(),
        CachedRepo {
            is_git,
            skill_files,
            dirs,
        },
    );
    Ok(NavigatedToDirectoryResponse {
        indexed_path: root,
        is_git,
    })
}

pub(super) async fn load_repo_metadata_directory(
    shell: &ShellBackend,
    repo_path: String,
    dir_path: String,
) -> Result<LoadRepoMetadataDirectoryResponse, BackendError> {
    let kind = shell.shell_kind();
    // One round trip: canonicalize both paths and list the directory, then judge afterwards.
    let script = new_script()?
        .section(
            "canon_repo",
            canonical_dir_command(kind, &repo_path),
            StderrMode::Discard,
        )?
        .section(
            "canon_dir",
            canonical_dir_command(kind, &dir_path),
            StderrMode::Discard,
        )?;
    let script = listing_script(script, kind, &dir_path, true, "")?;
    let frame = shell.run_script(&script).await?;

    let canonical = |name: &str, what: &str| -> Result<String, BackendError> {
        let section = frame
            .section(name)
            .filter(|s| s.success())
            .map(|s| s.text().trim().to_owned())
            .filter(|s| !s.is_empty());
        section.ok_or_else(|| {
            server_error(
                ErrorCode::InvalidRequest,
                format!("Invalid {what}: not a directory, or it cannot be read"),
            )
        })
    };
    let repo = canonical("canon_repo", "repo_path")?;
    let dir = canonical("canon_dir", "dir_path")?;
    if !is_within(&repo, &dir) {
        return Err(server_error(
            ErrorCode::InvalidRequest,
            format!("dir_path {dir} is not a descendant of repo_path {repo}"),
        ));
    }

    let mut cache = shell.tree_cache.lock();
    let Some(cached) = cache.get_mut(&repo) else {
        return Err(server_error(
            ErrorCode::Internal,
            format!("Failed to load directory: {repo} is not an indexed repository"),
        ));
    };
    let all_ignored = dir_is_ignored(&cached.dirs, &dir);
    let children = children_from_frame(&frame, "", cached.is_git, all_ignored).map_err(|m| {
        server_error(
            ErrorCode::Internal,
            format!("Failed to load directory: {m}"),
        )
    })?;
    cached.dirs.insert(dir.clone(), children.clone());
    let entries = if children.is_empty() {
        Vec::new()
    } else {
        vec![dir_update(&dir, &children, &|p| {
            cached.dirs.contains_key(p)
        })]
    };
    Ok(LoadRepoMetadataDirectoryResponse {
        repo_path,
        dir_path,
        entries,
    })
}

/// `ListDirectory` (family `fs_basic`, used by link validation) on the shell backend: the same
/// listing commands, answered in the helper's shape. Sizes and modification times are not read;
/// the directory entries carry names and kinds, sorted by name like the helper's.
pub(super) async fn list_directory(
    shell: &ShellBackend,
    path: String,
) -> Result<ListDirectoryResponse, BackendError> {
    let kind = shell.shell_kind();
    let script = new_script()?
        .section(
            "canon",
            canonical_dir_command(kind, &path),
            StderrMode::Discard,
        )?
        .section(
            "list",
            list_dir_stat_command(kind, &path),
            StderrMode::Discard,
        )?;
    let frame = shell.run_script(&script).await?;
    let failed = |why: &str| ListDirectoryResponse {
        result: Some(list_directory_response::Result::Error(FileOperationError {
            message: format!("Failed to list directory {path}: {why}"),
        })),
    };
    let canonical = frame
        .section("canon")
        .filter(|s| s.success())
        .map(|s| s.text().trim().to_owned())
        .filter(|s| !s.is_empty());
    let Some(canonical_path) = canonical else {
        return Ok(failed("not a directory, or it cannot be read"));
    };
    let Some(list) = frame.section("list").filter(|s| s.success()) else {
        return Ok(failed("the directory cannot be listed"));
    };
    let mut entries: Vec<DirEntry> = parse_stat_records(&list.output)
        .into_iter()
        .map(|c| {
            let kind = match c.kind {
                'd' => FileSystemEntryKind::Directory,
                'f' => FileSystemEntryKind::File,
                'l' => FileSystemEntryKind::Symlink,
                _ => FileSystemEntryKind::Other,
            };
            DirEntry {
                name: c.name,
                is_dir: kind == FileSystemEntryKind::Directory,
                kind: kind as i32,
                // Like the helper: a size for regular files only.
                size_bytes: (kind == FileSystemEntryKind::File).then_some(c.size),
                modified_epoch_millis: c.modified_epoch_millis,
            }
        })
        .collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(ListDirectoryResponse {
        result: Some(list_directory_response::Result::Success(
            ListDirectorySuccess {
                entries,
                canonical_path,
            },
        )),
    })
}

/// How many cached directories a non-git refresh re-lists in one call.
const MAX_REFRESH_DIRS: usize = 64;

/// Re-reads the tree of `repo_path` (an indexed root) and queues a [`ShellPush::RepoMetadataUpdate`]
/// with what changed since the last read. Nothing is queued when nothing changed or the root is
/// not indexed. The caller decides WHEN (prompt returned, cwd changed, user asked).
pub(super) async fn refresh_repo_tree(
    shell: &ShellBackend,
    repo_path: &str,
) -> Result<(), BackendError> {
    let (is_git, known): (bool, Vec<String>) = {
        let cache = shell.tree_cache.lock();
        let Some(cached) = cache.get(repo_path) else {
            return Ok(());
        };
        (cached.is_git, cached.dirs.keys().cloned().collect())
    };
    let fresh = if is_git {
        git_snapshot(shell, repo_path, None).await?
    } else {
        let kind = shell.shell_kind();
        let dirs: Vec<&String> = known.iter().take(MAX_REFRESH_DIRS).collect();
        let mut script = new_script()?;
        for (i, dir) in dirs.iter().enumerate() {
            script = listing_script(script, kind, dir, false, &i.to_string())?;
        }
        let frame = shell.run_script(&script).await?;
        let mut out = BTreeMap::new();
        for (i, dir) in dirs.iter().enumerate() {
            // A directory that can no longer be listed is gone: leave it out of `fresh` and its
            // parent's listing removes it.
            if let Ok(children) = children_from_frame(&frame, &i.to_string(), false, false) {
                out.insert((*dir).clone(), children);
            }
        }
        out
    };
    let mut cache = shell.tree_cache.lock();
    let Some(cached) = cache.get_mut(repo_path) else {
        return Ok(());
    };
    let (removed, updates) = apply_refresh(&mut cached.dirs, &fresh);
    let skill_files = agent_context::project_skill_files(&cached.dirs);
    let standing = agent_context::standing_delta(&cached.skill_files, &skill_files);
    cached.skill_files = skill_files;
    drop(cache);
    if !removed.is_empty() || !updates.is_empty() {
        shell.push(ShellPush::RepoMetadataUpdate(RepoMetadataUpdatePush {
            repo_path: repo_path.to_owned(),
            remove_entries: removed,
            update_entries: updates,
            standing_results_delta: standing,
        }));
    }
    Ok(())
}
