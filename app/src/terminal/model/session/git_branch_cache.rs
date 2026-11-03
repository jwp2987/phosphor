//! Per-session cache of the local branch list used by command corrections (#1342).
//!
//! Command corrections ask `git branch --no-color` after every finished block, even an empty
//! Enter; on Windows each copy is a process spawn. The list only changes when a ref is created,
//! deleted or `HEAD` moves, so the answer is reused while a stamp of those files is unchanged:
//! `HEAD`, `packed-refs`, and the modification time of every directory under `refs/heads` (a
//! branch is created or deleted by adding or removing a file in one of them). A stamp that
//! cannot be taken (no repository found, reftable, a huge refs tree) means no caching, so the
//! caller runs git exactly as before.
//!
//! The repository watcher's HEAD/branch-ref events bump a global epoch (`util::git::note_refs_changed`)
//! that is part of the stamp, so a live watcher invalidates the list on the event itself; the file
//! stamp remains the fallback for repositories no watcher covers.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use lazy_static::lazy_static;

use warp_core::SessionId;

/// Environment kill switch: set to disable the cache and run `git branch` per finished block.
pub(super) const DISABLE_ENV: &str = "PHOSPHOR_NO_BRANCH_LIST_CACHE";

/// Directories under `refs/heads` walked before giving up on a stamp.
const MAX_REF_DIRS: usize = 256;
const MAX_CACHED_ENTRIES: usize = 64;

type FileStamp = Option<(SystemTime, u64)>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct BranchStamp(Vec<(PathBuf, FileStamp)>, u64);

impl BranchStamp {
    #[cfg(test)]
    pub(super) fn from_entries(entries: Vec<(PathBuf, FileStamp)>) -> Self {
        Self(entries, 0)
    }

    #[cfg(test)]
    pub(super) fn with_epoch(mut self, epoch: u64) -> Self {
        self.1 = epoch;
        self
    }
}

fn file_stamp(path: &Path) -> FileStamp {
    std::fs::metadata(path)
        .ok()
        .map(|m| (m.modified().unwrap_or(SystemTime::UNIX_EPOCH), m.len()))
}

/// Resolves `(git_dir, common_dir)` for the repository containing `pwd`.
fn find_git_dirs(pwd: &Path) -> Option<(PathBuf, PathBuf)> {
    for dir in pwd.ancestors() {
        let dot_git = dir.join(".git");
        if dot_git.is_dir() {
            return Some((dot_git.clone(), dot_git));
        }
        if dot_git.is_file() {
            // A worktree or submodule: `gitdir: <path>`, with refs in the `commondir`.
            let text = std::fs::read_to_string(&dot_git).ok()?;
            let target = text.trim().strip_prefix("gitdir:")?.trim();
            let git_dir = dir.join(target);
            let common = match std::fs::read_to_string(git_dir.join("commondir")) {
                Ok(c) => git_dir.join(c.trim()),
                Err(_) => git_dir.clone(),
            };
            return Some((git_dir, common));
        }
    }
    None
}

/// Takes the stamp for the repository containing `pwd`, or `None` when it cannot be trusted.
pub(super) fn branch_stamp(pwd: &str) -> Option<BranchStamp> {
    // Read the watcher epoch before the files: a ref change landing mid-listing then leaves the
    // stored stamp behind the epoch, so the next lookup misses (never a stale hit).
    let epoch = crate::util::git::refs_epoch();
    let (git_dir, common) = find_git_dirs(Path::new(pwd))?;
    if common.join("reftable").exists() {
        return None;
    }
    let mut entries = vec![
        (git_dir.join("HEAD"), file_stamp(&git_dir.join("HEAD"))),
        (
            common.join("packed-refs"),
            file_stamp(&common.join("packed-refs")),
        ),
    ];
    let mut pending = vec![common.join("refs").join("heads")];
    let mut walked = 0;
    while let Some(dir) = pending.pop() {
        walked += 1;
        if walked > MAX_REF_DIRS {
            return None;
        }
        entries.push((dir.clone(), file_stamp(&dir)));
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read.flatten() {
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                pending.push(entry.path());
            }
        }
    }
    // Directory listing order is arbitrary; the stamp must not depend on it.
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    Some(BranchStamp(entries, epoch))
}

struct Entry {
    stamp: BranchStamp,
    branches: Vec<String>,
    /// Store order, so the oldest entry is the one evicted when the cache is full.
    seq: u64,
}

lazy_static! {
    static ref CACHE: parking_lot::Mutex<HashMap<(SessionId, String), Entry>> =
        parking_lot::Mutex::new(HashMap::new());
}

/// The cached branch list for `(session_id, pwd)`, if it was taken under `stamp`.
pub(super) fn lookup(session_id: SessionId, pwd: &str, stamp: &BranchStamp) -> Option<Vec<String>> {
    let cache = CACHE.lock();
    let entry = cache.get(&(session_id, pwd.to_owned()))?;
    (entry.stamp == *stamp).then(|| entry.branches.clone())
}

/// Records `branches`, which were listed while the repository had `stamp`.
pub(super) fn store(session_id: SessionId, pwd: &str, stamp: BranchStamp, branches: Vec<String>) {
    let mut cache = CACHE.lock();
    store_in(&mut cache, (session_id, pwd.to_owned()), stamp, branches);
}

type CacheMap = HashMap<(SessionId, String), Entry>;

/// Inserts, evicting only the oldest entry when full (the old behaviour cleared every session's
/// list at once, so a 65th directory made all the others re-run git; #1382).
fn store_in(
    cache: &mut CacheMap,
    key: (SessionId, String),
    stamp: BranchStamp,
    branches: Vec<String>,
) {
    static NEXT_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = NEXT_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if cache.len() >= MAX_CACHED_ENTRIES && !cache.contains_key(&key) {
        if let Some(oldest) = cache
            .iter()
            .min_by_key(|(_, e)| e.seq)
            .map(|(k, _)| k.clone())
        {
            cache.remove(&oldest);
        }
    }
    cache.insert(
        key,
        Entry {
            stamp,
            branches,
            seq,
        },
    );
}

#[cfg(test)]
#[path = "git_branch_cache_tests.rs"]
mod tests;
