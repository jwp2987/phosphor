//! Repo metadata and the lazy file tree (Wave H parity, family package P3, family `b`).
//!
//! The contract is the old app daemon's (`server_model.rs` `handle_navigated_to_directory` and
//! `handle_load_repo_metadata_directory`), reproduced here without warpui:
//!
//! * `NavigatedToDirectory { path }`: canonicalize `path` (failure: `InvalidRequest`
//!   "Invalid path: ..."), find the enclosing git repository by walking up to a `.git`, reply
//!   `{ indexed_path, is_git }` (the repo root, or the directory itself outside git), and THEN push a
//!   `RepoMetadataSnapshot` to the requesting connection, so the snapshot always lands before the
//!   client's first `LoadRepoMetadataDirectory`. A repo already indexed is not walked again; a
//!   connection that already received its snapshot is not sent another.
//! * `LoadRepoMetadataDirectory { repo_path, dir_path }`: canonicalize both (errors name the bad
//!   field), require `dir_path` under `repo_path`, list that one directory and reply with its
//!   children as one entry update (empty when the directory is empty).
//! * Pushes after that: a `RepoMetadataUpdatePush` whenever a watched directory's listing changes.
//!
//! The tree is walked with `ignore` (git ignore rules, no `.ignore` files). A git repository is
//! walked breadth-first up to [`MAX_SNAPSHOT_ENTRIES`] entries and the rest stays unloaded for lazy
//! loading; a directory outside git is listed one level deep, as the old server did. Ignored
//! directories and `.git` are listed (flagged) but never walked.
//!
//! Live updates come from the shared [`WatchSet`]: one non-recursive watch per loaded,
//! non-ignored directory. An `FsChange` marks the affected directories dirty; ONE relist runs on a
//! worker at a time and anything arriving meanwhile sets the dirty flag again (no timer). If a
//! watch cannot be had (cap, ENOSPC, NFS) the repo is marked unwatched: it keeps serving but never
//! pushes, is re-walked on the next `NavigatedToDirectory`, and the client falls back to
//! refreshing on prompt events (design ruling R5).
//!
//! Every indexed root is recorded ([`RepoTrees::roots`]) for the code review and link
//! validation features the old server registered them with.
//!
//! Standing-query results (project skills and rules discovered inside the repo) are NOT computed
//! here; they belong to the agent-context package.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use crate::proto::{
    ErrorCode, LoadRepoMetadataDirectory, LoadRepoMetadataDirectoryResponse, NavigatedToDirectory,
    NavigatedToDirectoryResponse, RepoMetadataEntryUpdate, RepoMetadataSnapshot,
    RepoMetadataUpdatePush, server_message,
};
use crate::shellcmd::tree::{
    TreeChild, diff_dir, dir_update, is_within, parent_of, snapshot_entries,
};

use super::jobs::Reply;
use super::server::{ConnId, Event, Pusher};
use super::watch::{FsKind, WatchSet};

/// The capability this module advertises.
#[allow(dead_code)]
pub(super) const CAPABILITY: &str = crate::capability::REPO_METADATA;

/// Entries a repository snapshot may hold before the remaining directories stay unloaded.
pub(super) const MAX_SNAPSHOT_ENTRIES: usize = 20_000;

fn lossy(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

// ---------------------------------------------------------------- blocking filesystem work

/// The children of `dir`. `in_git` applies git ignore rules (and flags `.git`); `all_ignored`
/// flags every child, for a directory that is itself inside an ignored one. Symlinks to
/// directories, sockets, fifos and devices are omitted.
pub(super) fn list_dir(dir: &Path, in_git: bool, all_ignored: bool) -> io::Result<Vec<TreeChild>> {
    let mut children = Vec::new();
    for entry in std::fs::read_dir(dir)?.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let is_dir = if file_type.is_symlink() {
            match std::fs::metadata(entry.path()) {
                Ok(target) if target.is_dir() => continue,
                _ => false,
            }
        } else if file_type.is_dir() {
            true
        } else if file_type.is_file() {
            false
        } else {
            continue;
        };
        children.push(TreeChild {
            name: entry.file_name().to_string_lossy().into_owned(),
            is_dir,
            ignored: false,
        });
    }
    if in_git {
        let visible = visible_names(dir);
        for c in &mut children {
            c.ignored = all_ignored || c.name == ".git" || !visible.contains(&c.name);
        }
    } else if all_ignored {
        for c in &mut children {
            c.ignored = true;
        }
    }
    crate::shellcmd::tree::sort_children(&mut children);
    Ok(children)
}

/// Names directly under `dir` that git's ignore rules do NOT hide.
fn visible_names(dir: &Path) -> HashSet<String> {
    let walker = ignore::WalkBuilder::new(dir)
        .max_depth(Some(1))
        .hidden(false)
        .parents(true)
        .ignore(false)
        .git_global(false)
        .require_git(false)
        .build();
    walker
        .flatten()
        .filter(|e| e.depth() == 1)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect()
}

/// The nearest ancestor (or `dir` itself) holding a `.git` directory or file.
pub(super) fn find_repo_root(dir: &Path) -> Option<PathBuf> {
    dir.ancestors()
        .find(|a| a.join(".git").exists())
        .map(Path::to_path_buf)
}

/// Walks `root` for a snapshot: breadth-first and capped for a git repository, one level for
/// anything else. Keys are absolute directory paths.
fn walk(root: &Path, is_git: bool) -> BTreeMap<String, Vec<TreeChild>> {
    let mut dirs = BTreeMap::new();
    let mut queue = VecDeque::from([root.to_path_buf()]);
    let mut total = 0usize;
    while let Some(dir) = queue.pop_front() {
        if total >= MAX_SNAPSHOT_ENTRIES && dir != root {
            // Over budget: this directory (and the ones after it) stay unloaded.
            continue;
        }
        let Ok(children) = list_dir(&dir, is_git, false) else {
            continue;
        };
        total += children.len();
        if is_git {
            for c in &children {
                if c.is_dir && !c.ignored {
                    queue.push_back(dir.join(&c.name));
                }
            }
        }
        dirs.insert(lossy(&dir), children);
    }
    dirs
}

// ---------------------------------------------------------------- state

/// One indexed root.
struct Repo {
    is_git: bool,
    /// Known listings by absolute directory path.
    dirs: BTreeMap<String, Vec<TreeChild>>,
    /// Directories holding a watch reference.
    held: BTreeSet<String>,
    watched: bool,
    subscribers: HashSet<ConnId>,
    dirty: BTreeSet<String>,
    in_flight: bool,
}

impl Repo {
    /// Whether `dir` sits inside an ignored directory (its parent's listing flags it).
    fn dir_is_ignored(&self, dir: &str) -> bool {
        crate::shellcmd::tree::dir_is_ignored(&self.dirs, dir)
    }

    /// Starts watching `dir` unless it is ignored; marks the repo unwatched if that fails.
    fn hold(&mut self, dir: &str, ignored: bool, watches: &mut WatchSet, root: &str) {
        if !self.watched || ignored || self.held.contains(dir) {
            return;
        }
        match watches.acquire(Path::new(dir)) {
            Ok(()) => {
                self.held.insert(dir.to_owned());
            }
            Err(e) => {
                log::warn!("repo {root} is unwatched: {e}");
                self.mark_unwatched(watches);
            }
        }
    }

    fn mark_unwatched(&mut self, watches: &mut WatchSet) {
        self.watched = false;
        self.release_all(watches);
        self.dirty.clear();
    }

    fn release_all(&mut self, watches: &mut WatchSet) {
        for dir in std::mem::take(&mut self.held) {
            watches.release(Path::new(&dir));
        }
    }

    /// Forgets `dir` and every listing below it, releasing their watches.
    fn drop_subtree(&mut self, dir: &str, watches: &mut WatchSet) {
        let below: Vec<String> = self
            .dirs
            .keys()
            .filter(|k| is_within(dir, k))
            .cloned()
            .collect();
        for k in below {
            self.dirs.remove(&k);
            self.dirty.remove(&k);
            if self.held.remove(&k) {
                watches.release(Path::new(&k));
            }
        }
    }
}

/// A finished walk, reported back to the core by a worker.
pub(super) struct Indexed {
    conn: ConnId,
    reply: Reply,
    root: String,
    is_git: bool,
    dirs: BTreeMap<String, Vec<TreeChild>>,
}

/// A finished relist of dirty directories (`None`: the directory is gone or unreadable).
pub(super) struct Relisted {
    root: String,
    results: Vec<(String, Option<Vec<TreeChild>>)>,
}

/// All indexed roots. Owned by the server core.
#[derive(Default)]
pub(super) struct RepoTrees {
    repos: HashMap<String, Repo>,
}

impl RepoTrees {
    /// Every indexed root, for features that need to know which roots the host has registered
    /// (code review, link validation, the diff-state package).
    #[allow(dead_code)] // consumed by the packages that follow
    pub(super) fn roots(&self) -> Vec<String> {
        self.repos.keys().cloned().collect()
    }

    #[allow(dead_code)]
    pub(super) fn is_known_root(&self, root: &str) -> bool {
        self.repos.contains_key(root)
    }

    pub(super) fn navigate(
        &mut self,
        conn: ConnId,
        msg: NavigatedToDirectory,
        reply: Reply,
        events: Option<&Sender<Event>>,
        watches: &mut WatchSet,
        push: &Pusher<'_>,
    ) {
        log::info!("Handling NavigatedToDirectory path={}", msg.path);
        let canonical = match std::fs::canonicalize(&msg.path) {
            Ok(p) if p.is_dir() => p,
            Ok(_) => {
                reply.error(
                    ErrorCode::InvalidRequest,
                    "Invalid path: not a directory".to_string(),
                );
                return;
            }
            Err(e) => {
                log::warn!("Invalid path for NavigatedToDirectory: {e}");
                reply.error(ErrorCode::InvalidRequest, format!("Invalid path: {e}"));
                return;
            }
        };
        let git_root = find_repo_root(&canonical);
        let is_git = git_root.is_some();
        let root = git_root.unwrap_or(canonical);
        let root_s = lossy(&root);

        // Already indexed and still live: no walk.
        if let Some(repo) = self.repos.get_mut(&root_s)
            && repo.watched
        {
            reply.send(server_message::Message::NavigatedToDirectoryResponse(
                NavigatedToDirectoryResponse {
                    indexed_path: root_s.clone(),
                    is_git,
                },
            ));
            if repo.subscribers.insert(conn) {
                push.send(conn, snapshot_message(&root_s, &repo.dirs));
            }
            return;
        }

        let job = move || {
            let dirs = walk(&root, is_git);
            Indexed {
                conn,
                reply,
                root: root_s,
                is_git,
                dirs,
            }
        };
        match events {
            Some(tx) => {
                let tx = tx.clone();
                let spawned =
                    std::thread::Builder::new()
                        .name("repo-walk".into())
                        .spawn(move || {
                            let _ = tx.send(Event::RepoIndexed(Box::new(job())));
                        });
                if let Err(e) = spawned {
                    log::error!("could not start a worker thread: {e}");
                }
            }
            // Unit tests that drive the core by hand have no event channel: walk inline.
            None => self.indexed(job(), watches, push),
        }
    }

    /// A walk finished: register the root, answer the request, then push the snapshot.
    pub(super) fn indexed(&mut self, done: Indexed, watches: &mut WatchSet, push: &Pusher<'_>) {
        let Indexed {
            conn,
            reply,
            root,
            is_git,
            dirs,
        } = done;
        let mut subscribers = HashSet::new();
        if let Some(mut old) = self.repos.remove(&root) {
            old.release_all(watches);
            subscribers = old.subscribers;
        }
        subscribers.insert(conn);
        let mut repo = Repo {
            is_git,
            dirs,
            held: BTreeSet::new(),
            watched: true,
            subscribers,
            dirty: BTreeSet::new(),
            in_flight: false,
        };
        let loaded: Vec<String> = repo.dirs.keys().cloned().collect();
        for dir in loaded {
            let ignored = repo.dir_is_ignored(&dir);
            repo.hold(&dir, ignored, watches, &root);
        }
        let message = snapshot_message(&root, &repo.dirs);
        self.repos.insert(root.clone(), repo);
        log::info!("Indexed {root} (git={is_git})");
        reply.send(server_message::Message::NavigatedToDirectoryResponse(
            NavigatedToDirectoryResponse {
                indexed_path: root,
                is_git,
            },
        ));
        push.send(conn, message);
    }

    pub(super) fn load_directory(
        &mut self,
        msg: LoadRepoMetadataDirectory,
        reply: &Reply,
        watches: &mut WatchSet,
    ) {
        log::info!(
            "Handling LoadRepoMetadataDirectory repo_path={} dir_path={}",
            msg.repo_path,
            msg.dir_path
        );
        let repo_path = match std::fs::canonicalize(&msg.repo_path) {
            Ok(p) => p,
            Err(e) => {
                reply.error(ErrorCode::InvalidRequest, format!("Invalid repo_path: {e}"));
                return;
            }
        };
        let dir_path = match std::fs::canonicalize(&msg.dir_path) {
            Ok(p) => p,
            Err(e) => {
                reply.error(ErrorCode::InvalidRequest, format!("Invalid dir_path: {e}"));
                return;
            }
        };
        if !dir_path.starts_with(&repo_path) {
            reply.error(
                ErrorCode::InvalidRequest,
                format!(
                    "dir_path {} is not a descendant of repo_path {}",
                    dir_path.display(),
                    repo_path.display()
                ),
            );
            return;
        }
        let root = lossy(&repo_path);
        let dir = lossy(&dir_path);
        let Some(repo) = self.repos.get_mut(&root) else {
            reply.error(
                ErrorCode::Internal,
                format!("Failed to load directory: {root} is not an indexed repository"),
            );
            return;
        };
        let ignored = repo.dir_is_ignored(&dir);
        let children = match list_dir(&dir_path, repo.is_git, ignored) {
            Ok(c) => c,
            Err(e) => {
                reply.error(
                    ErrorCode::Internal,
                    format!("Failed to load directory: {e}"),
                );
                return;
            }
        };
        repo.dirs.insert(dir.clone(), children.clone());
        repo.hold(&dir, ignored, watches, &root);
        let entries = if children.is_empty() {
            Vec::new()
        } else {
            vec![dir_update(&dir, &children, &|p| repo.dirs.contains_key(p))]
        };
        reply.send(server_message::Message::LoadRepoMetadataDirectoryResponse(
            LoadRepoMetadataDirectoryResponse {
                repo_path: msg.repo_path,
                dir_path: msg.dir_path,
                entries,
            },
        ));
    }

    /// The watcher saw a change: mark the directories whose listings it can have changed.
    pub(super) fn on_fs_change(
        &mut self,
        paths: &[PathBuf],
        kind: FsKind,
        events: Option<&Sender<Event>>,
    ) {
        for (root, repo) in &mut self.repos {
            if !repo.watched {
                continue;
            }
            if kind == FsKind::Rescan {
                repo.dirty.extend(repo.held.iter().cloned());
            }
            for path in paths {
                let p = lossy(path);
                if !is_within(root, &p) {
                    continue;
                }
                let gitignore = path.file_name().is_some_and(|n| n == ".gitignore");
                if kind == FsKind::Modify && !gitignore {
                    // A content change never changes a listing.
                    continue;
                }
                if gitignore {
                    // Ignore flags of everything below may have flipped.
                    repo.dirty.extend(repo.held.iter().cloned());
                    continue;
                }
                if let Some(parent) = parent_of(&p)
                    && repo.held.contains(parent)
                {
                    repo.dirty.insert(parent.to_owned());
                }
                // The directory itself (renamed away or removed).
                if repo.held.contains(&p) {
                    repo.dirty.insert(p);
                }
            }
            start_relist(root, repo, events);
        }
    }

    /// A relist finished: diff it against what is cached and push the difference.
    pub(super) fn relisted(
        &mut self,
        done: Relisted,
        events: Option<&Sender<Event>>,
        watches: &mut WatchSet,
        push: &Pusher<'_>,
    ) {
        let Some(repo) = self.repos.get_mut(&done.root) else {
            return;
        };
        repo.in_flight = false;
        let mut removed: BTreeSet<String> = BTreeSet::new();
        let mut updates: Vec<RepoMetadataEntryUpdate> = Vec::new();
        for (dir, result) in done.results {
            let Some(old) = repo.dirs.get(&dir).cloned() else {
                continue;
            };
            match result {
                None => {
                    repo.drop_subtree(&dir, watches);
                    removed.insert(dir);
                }
                Some(new) => {
                    let (gone, added) = diff_dir(&dir, &old, &new);
                    for path in gone {
                        repo.drop_subtree(&path, watches);
                        removed.insert(path);
                    }
                    // Newly ignored or un-ignored directories change whether they are watched.
                    repo.dirs.insert(dir.clone(), new);
                    if !added.is_empty() {
                        updates.push(RepoMetadataEntryUpdate {
                            parent_path_to_replace: dir,
                            subtree_metadata: added,
                        });
                    }
                }
            }
        }
        // A directory removed together with an ancestor needs no separate removal.
        let removed: Vec<String> = removed.into_iter().collect();
        if !removed.is_empty() || !updates.is_empty() {
            let message = server_message::Message::RepoMetadataUpdate(RepoMetadataUpdatePush {
                repo_path: done.root.clone(),
                remove_entries: removed,
                update_entries: updates,
                standing_results_delta: None,
            });
            for conn in &repo.subscribers {
                push.send(*conn, message.clone());
            }
        }
        start_relist(&done.root, repo, events);
    }

    /// A connection went away: forget it, and drop roots nobody follows (releasing their watches).
    pub(super) fn conn_gone(&mut self, conn: ConnId, watches: &mut WatchSet) {
        for repo in self.repos.values_mut() {
            repo.subscribers.remove(&conn);
        }
        let orphaned: Vec<String> = self
            .repos
            .iter()
            .filter(|(_, r)| r.subscribers.is_empty())
            .map(|(k, _)| k.clone())
            .collect();
        for root in orphaned {
            if let Some(mut repo) = self.repos.remove(&root) {
                repo.release_all(watches);
            }
        }
    }
}

fn snapshot_message(
    root: &str,
    dirs: &BTreeMap<String, Vec<TreeChild>>,
) -> server_message::Message {
    server_message::Message::RepoMetadataSnapshot(RepoMetadataSnapshot {
        repo_path: root.to_owned(),
        entries: snapshot_entries(root, dirs),
        sync_complete: true,
        standing_results: None,
    })
}

/// Starts the one relist a repo may have in flight, if anything is dirty.
fn start_relist(root: &str, repo: &mut Repo, events: Option<&Sender<Event>>) {
    if repo.in_flight || repo.dirty.is_empty() || !repo.watched {
        return;
    }
    let Some(tx) = events.cloned() else {
        repo.dirty.clear();
        return;
    };
    let dirty: Vec<(String, bool)> = std::mem::take(&mut repo.dirty)
        .into_iter()
        .map(|d| {
            let ignored = repo.dir_is_ignored(&d);
            (d, ignored)
        })
        .collect();
    let is_git = repo.is_git;
    let root = root.to_owned();
    repo.in_flight = true;
    let spawned = std::thread::Builder::new()
        .name("repo-relist".into())
        .spawn(move || {
            let results = dirty
                .into_iter()
                .map(|(dir, ignored)| {
                    let listing = list_dir(Path::new(&dir), is_git, ignored).ok();
                    (dir, listing)
                })
                .collect();
            let _ = tx.send(Event::RepoRelisted(Relisted { root, results }));
        });
    if let Err(e) = spawned {
        log::error!("could not start a worker thread: {e}");
        repo.in_flight = false;
    }
}

#[cfg(test)]
#[path = "repo_meta_tests.rs"]
mod tests;
