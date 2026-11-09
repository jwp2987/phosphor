//! What the helper watches for a repository whose git-derived state (diff state, git status) it
//! serves, and how a filesystem event turns into "this file changed" or "everything may have".
//!
//! One [`Watched`] per repository root, refcounted by its users (diff keys and status
//! subscribers), over the shared [`WatchSet`]. The watches are non-recursive, so the set is:
//! the root, the git directory (HEAD, index), `logs` (every HEAD move appends to `logs/HEAD`),
//! every directory under `refs`, and one directory per tracked file's parent. Directories that
//! hold changed files are added after each compute ([`RepoWatches::add_dirs`]). The directory set
//! is capped; past the cap the repo keeps the watches it has and says so in the log (design ruling
//! R5), and a watch the OS refuses for capacity reasons (inotify limit, ENOSPC) marks the repo
//! unwatched: it keeps answering requests but stops pushing, and the client's explicit refresh
//! still works.
//!
//! No timer anywhere: an event marks state dirty and the consumers run one recompute at a time.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use super::git_runner::HelperGitRunner;
use super::jobs::spawn_worker;
use super::server::Event;
use super::watch::{FsKind, WatchSet};
use crate::shellcmd::git::git_text;

/// Tracked-file parent directories watched per repository. Past this the repo is partly watched.
pub(super) const MAX_TRACKED_DIRS: usize = 3_000;
/// Directories under `refs` that are watched.
const MAX_REF_DIRS: usize = 300;
/// More changed files than this in one batch are recomputed as a whole snapshot.
pub(super) const MAX_FILE_DELTAS: usize = 50;

/// What a batch of filesystem events changed in one repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Change {
    /// HEAD, the index, a ref, `.gitignore`, a new directory or too many files: recompute
    /// everything.
    All,
    /// These repo-relative files changed.
    Files(Vec<String>),
}

/// The directories to watch for a repository, planned by a worker.
#[derive(Debug, Default)]
pub(super) struct WatchPlan {
    pub dirs: Vec<PathBuf>,
    /// The git directory and the common directory (they differ in a linked worktree).
    pub git_dirs: Vec<PathBuf>,
    /// The tracked directories did not all fit under the cap.
    pub truncated: bool,
}

fn lossy(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Plans the watches for `root` (blocking: runs `git`). Never fails: whatever could not be
/// learned is simply not watched.
pub(super) fn plan_watches(root: &Path) -> WatchPlan {
    let runner = HelperGitRunner::default();
    let mut plan = WatchPlan {
        dirs: vec![root.to_path_buf()],
        ..WatchPlan::default()
    };
    if let Ok(out) = futures_lite::future::block_on(git_text(
        &runner,
        root,
        &["rev-parse", "--git-dir", "--git-common-dir"],
    )) {
        for line in out.lines().map(str::trim).filter(|l| !l.is_empty()).take(2) {
            let joined = root.join(line);
            let dir = std::fs::canonicalize(&joined).unwrap_or(joined);
            if !plan.git_dirs.contains(&dir) {
                plan.git_dirs.push(dir);
            }
        }
    }
    if plan.git_dirs.is_empty() {
        plan.git_dirs.push(root.join(".git"));
    }
    for git_dir in plan.git_dirs.clone() {
        plan.dirs.push(git_dir.clone());
        let logs = git_dir.join("logs");
        if logs.is_dir() {
            plan.dirs.push(logs);
        }
        walk_dirs(&git_dir.join("refs"), &mut plan.dirs, MAX_REF_DIRS);
    }
    if let Ok(out) = futures_lite::future::block_on(git_text(&runner, root, &["ls-files", "-z"])) {
        let mut parents: BTreeSet<PathBuf> = BTreeSet::new();
        for file in out.split('\0').filter(|f| !f.is_empty()) {
            if let Some(parent) = Path::new(file).parent() {
                parents.insert(root.join(parent));
            }
        }
        parents.remove(root);
        if parents.len() > MAX_TRACKED_DIRS {
            plan.truncated = true;
        }
        plan.dirs.extend(parents.into_iter().take(MAX_TRACKED_DIRS));
    }
    plan
}

/// `dir` and every directory below it, up to `cap` of them.
fn walk_dirs(dir: &Path, out: &mut Vec<PathBuf>, cap: usize) {
    let mut stack = vec![dir.to_path_buf()];
    let mut taken = 0;
    while let Some(d) = stack.pop() {
        if taken >= cap {
            return;
        }
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        out.push(d.clone());
        taken += 1;
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                stack.push(entry.path());
            }
        }
    }
}

struct Watched {
    root: PathBuf,
    users: usize,
    git_dirs: Vec<PathBuf>,
    held: BTreeSet<PathBuf>,
    unwatched: bool,
}

impl Watched {
    fn hold(&mut self, dir: &Path, watches: &mut WatchSet) {
        if self.unwatched || self.held.contains(dir) {
            return;
        }
        match watches.acquire(dir) {
            Ok(()) => {
                self.held.insert(dir.to_path_buf());
            }
            // The directory vanished between planning and watching: not a reason to give up.
            Err(e) if !e.is_capacity() => log::debug!("not watching {}: {e}", dir.display()),
            Err(e) => {
                log::warn!(
                    "repo {} is unwatched: {e}; it keeps answering but pushes nothing",
                    self.root.display()
                );
                self.unwatched = true;
                self.release_all(watches);
            }
        }
    }

    fn release_all(&mut self, watches: &mut WatchSet) {
        for dir in std::mem::take(&mut self.held) {
            watches.release(&dir);
        }
    }

    /// What a change at `path` means for this repository, if anything.
    fn classify_path(&self, path: &Path) -> Option<Change> {
        for git_dir in &self.git_dirs {
            if let Ok(rel) = path.strip_prefix(git_dir) {
                return git_internal(rel);
            }
        }
        let rel = path.strip_prefix(&self.root).ok()?;
        if rel.as_os_str().is_empty() {
            return None;
        }
        // Inside a nested `.git` (a submodule): not ours.
        if rel.components().any(|c| c.as_os_str() == ".git") {
            return None;
        }
        if rel.file_name().is_some_and(|n| n == ".gitignore")
            || self.held.contains(path)
            || path.is_dir()
        {
            // Ignore rules moved, or a directory appeared/vanished: the watch set and the file
            // list both need a fresh look.
            return Some(Change::All);
        }
        Some(Change::Files(vec![lossy(rel)]))
    }
}

/// A change inside the git directory: only HEAD moves, the index, refs and the HEAD log matter.
fn git_internal(rel: &Path) -> Option<Change> {
    let name = rel.file_name()?.to_string_lossy();
    if name.ends_with(".lock") {
        // Recomputing against a locked index reads half-written state; the lock's release
        // arrives as a change to the real file.
        return None;
    }
    let s = rel.to_string_lossy();
    let relevant = matches!(
        &*s,
        "HEAD" | "index" | "packed-refs" | "ORIG_HEAD" | "logs/HEAD"
    ) || s.starts_with("refs/");
    relevant.then_some(Change::All)
}

/// All repositories with live watches. Owned by the server core.
#[derive(Default)]
pub(super) struct RepoWatches {
    repos: HashMap<String, Watched>,
}

impl RepoWatches {
    /// One more user of `root`. The first starts the watches on the root and the git directory at
    /// once and plans the rest on a worker (delivered as [`Event::RepoWatchPlan`]).
    pub(super) fn acquire(
        &mut self,
        root: &str,
        events: Option<&Sender<Event>>,
        watches: &mut WatchSet,
    ) {
        if let Some(w) = self.repos.get_mut(root) {
            w.users += 1;
            return;
        }
        let root_path = PathBuf::from(root);
        let mut w = Watched {
            root: root_path.clone(),
            users: 1,
            git_dirs: vec![root_path.join(".git")],
            held: BTreeSet::new(),
            unwatched: false,
        };
        w.hold(&root_path, watches);
        w.hold(&root_path.join(".git"), watches);
        self.repos.insert(root.to_owned(), w);
        if let Some(tx) = events.cloned() {
            let root = root.to_owned();
            spawn_worker("repo-watch-plan", move || {
                let plan = plan_watches(Path::new(&root));
                let _ = tx.send(Event::RepoWatchPlan { root, plan });
            });
        }
    }

    /// One user fewer; the last one releases every watch.
    pub(super) fn release(&mut self, root: &str, watches: &mut WatchSet) {
        let Some(w) = self.repos.get_mut(root) else {
            return;
        };
        w.users = w.users.saturating_sub(1);
        if w.users == 0 {
            w.release_all(watches);
            self.repos.remove(root);
        }
    }

    /// A worker planned the watches: hold them.
    pub(super) fn apply_plan(&mut self, root: &str, plan: WatchPlan, watches: &mut WatchSet) {
        let Some(w) = self.repos.get_mut(root) else {
            return;
        };
        if plan.truncated {
            log::warn!(
                "repo {root}: more than {MAX_TRACKED_DIRS} tracked directories; the rest are not watched"
            );
        }
        w.git_dirs = plan.git_dirs;
        for dir in plan.dirs {
            w.hold(&dir, watches);
        }
    }

    /// Also watch `dirs` (the directories of files that just showed up as changed).
    pub(super) fn add_dirs(
        &mut self,
        root: &str,
        dirs: impl IntoIterator<Item = PathBuf>,
        watches: &mut WatchSet,
    ) {
        let Some(w) = self.repos.get_mut(root) else {
            return;
        };
        for dir in dirs {
            if w.held.len() >= MAX_TRACKED_DIRS + MAX_REF_DIRS * 2 + 8 {
                return;
            }
            w.hold(&dir, watches);
        }
    }

    /// Whether no repository is followed.
    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.repos.is_empty()
    }

    /// Whether `root` could not be watched (capacity), so nothing will be pushed for it.
    #[allow(dead_code)] // for the consumers' logs and tests
    pub(super) fn is_unwatched(&self, root: &str) -> bool {
        self.repos.get(root).is_some_and(|w| w.unwatched)
    }

    /// Turns one batch of watcher events into per-repository changes.
    pub(super) fn classify(&self, paths: &[PathBuf], kind: FsKind) -> Vec<(String, Change)> {
        let mut out = Vec::new();
        for (root, w) in &self.repos {
            if w.unwatched {
                continue;
            }
            if kind == FsKind::Rescan {
                out.push((root.clone(), Change::All));
                continue;
            }
            let mut all = false;
            let mut files: BTreeSet<String> = BTreeSet::new();
            for path in paths {
                match w.classify_path(path) {
                    Some(Change::All) => all = true,
                    Some(Change::Files(f)) => files.extend(f),
                    None => {}
                }
            }
            if all || files.len() > MAX_FILE_DELTAS {
                out.push((root.clone(), Change::All));
            } else if !files.is_empty() {
                out.push((root.clone(), Change::Files(files.into_iter().collect())));
            }
        }
        out
    }
}

#[cfg(test)]
#[path = "repo_watch_tests.rs"]
mod tests;
