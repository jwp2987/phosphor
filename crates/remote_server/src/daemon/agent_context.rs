//! The agent-context snapshot (host skills and global rule files; never bundled skills, design
//! ruling R3), pushed under `crate::capability::AGENT_CONTEXT` (Wave H parity, family package P8,
//! family `h`). The snapshot is a push, so there is no request arm.
//!
//! * What is scanned, how it is filtered and what the snapshot looks like are the shared
//!   [`crate::shellcmd::agent_context`] (the shell backend assembles the same snapshot).
//! * On `Initialize` a connection gets the current snapshot (the first connection starts the
//!   first scan; the result is pushed to every connection).
//! * The skill roots, the provider directories and the home directory are watched through the
//!   shared [`WatchSet`]. A relevant `FsChange` starts ONE scan on a worker thread (a dirty flag
//!   queues one more if events arrive meanwhile; no timer). A scan that finds a different
//!   context bumps the revision and is broadcast; an unchanged scan says nothing.
//! * If a watch cannot be had (cap, no watcher), the context is marked unwatched: it is rescanned
//!   whenever a connection initializes, which is when the client would otherwise be stale.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use crate::proto::{RemoteAgentContextSnapshot, server_message};
use crate::shellcmd::agent_context::{
    self as spec, HOME_RULE_FILES, HOME_SKILL_DIRS, MAX_FILE_BYTES, MAX_SKILLS, MAX_TOTAL_BYTES,
    RawFile,
};

use super::server::{Event, Pusher};
use super::watch::{FsKind, WatchSet};

/// The capability this module advertises.
#[allow(dead_code)]
pub(super) const CAPABILITY: &str = crate::capability::AGENT_CONTEXT;

/// What one scan found.
#[derive(Debug)]
pub(super) struct Scanned {
    home_dir: String,
    skills: Vec<RawFile>,
    rules: Vec<RawFile>,
    /// The directories to watch, so a change in any of them reaches [`AgentContext::on_fs_change`].
    watch_dirs: BTreeSet<PathBuf>,
}

fn lossy(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Reads a regular file within the size and budget caps; `None` for anything else.
fn read_capped(path: &Path, budget: &mut usize) -> Option<Vec<u8>> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    let len = usize::try_from(meta.len()).ok()?;
    if len > MAX_FILE_BYTES {
        return None;
    }
    *budget += len;
    if *budget > MAX_TOTAL_BYTES {
        return None;
    }
    std::fs::read(path).ok()
}

/// Scans `home` with `std::fs` only: for each root in [`HOME_SKILL_DIRS`] the `*/SKILL.md` one
/// level down (names sorted, dot-directories skipped, as the shell's `*` glob does), and the
/// [`HOME_RULE_FILES`].
pub(super) fn scan(home: &Path) -> Scanned {
    let mut watch_dirs = BTreeSet::from([home.to_path_buf()]);
    let mut used = 0usize;
    let mut skills = Vec::new();
    for root in HOME_SKILL_DIRS {
        let dir = home.join(root);
        // The provider directory (`~/.claude`) is watched so the appearance of its `skills`
        // directory is seen.
        if let Some(provider) = dir.parent()
            && provider.is_dir()
        {
            watch_dirs.insert(provider.to_path_buf());
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        watch_dirs.insert(dir.clone());
        let mut names: Vec<_> = entries
            .filter_map(Result::ok)
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| !n.starts_with('.'))
            .collect();
        names.sort();
        for name in names {
            let skill_dir = dir.join(&name);
            if !skill_dir.is_dir() {
                continue;
            }
            watch_dirs.insert(skill_dir.clone());
            let file = skill_dir.join("SKILL.md");
            if let Some(bytes) = read_capped(&file, &mut used) {
                skills.push(RawFile {
                    path: lossy(&file),
                    bytes,
                });
            }
        }
    }
    let mut rules = Vec::new();
    for rule in HOME_RULE_FILES {
        let file = home.join(rule);
        if let Some(bytes) = read_capped(&file, &mut used) {
            rules.push(RawFile {
                path: lossy(&file),
                bytes,
            });
        }
    }
    Scanned {
        home_dir: lossy(home),
        skills,
        rules,
        watch_dirs,
    }
}

/// The helper's agent context: the last snapshot sent, its revision, and the scan state.
pub(super) struct AgentContext {
    home: Option<PathBuf>,
    revision: u64,
    current: Option<RemoteAgentContextSnapshot>,
    scanning: bool,
    dirty: bool,
    /// Every directory of the last scan holds a watch reference.
    watched: bool,
    held: BTreeSet<PathBuf>,
}

impl AgentContext {
    pub(super) fn new(home: Option<PathBuf>) -> Self {
        Self {
            home,
            revision: 0,
            current: None,
            scanning: false,
            dirty: false,
            watched: false,
            held: BTreeSet::new(),
        }
    }

    /// The snapshot last published, if a scan has finished.
    #[cfg(test)]
    pub(super) fn current(&self) -> Option<&RemoteAgentContextSnapshot> {
        self.current.as_ref()
    }

    /// A connection initialized: it gets the current snapshot right away (after its
    /// `InitializeResponse`); with none yet, or when changes are not being watched, a scan starts
    /// and its result reaches every connection.
    pub(super) fn on_connect(
        &mut self,
        conn: super::server::ConnId,
        events: Option<&Sender<Event>>,
        watches: &mut WatchSet,
        push: &Pusher<'_>,
    ) {
        if let Some(current) = &self.current {
            push.send(
                conn,
                server_message::Message::RemoteAgentContextSnapshot(current.clone()),
            );
        }
        if self.current.is_none() || !self.watched {
            self.request_scan(events, watches, push);
        }
    }

    /// A filesystem change: scan again when it touches a skill root, a provider directory or a
    /// rule file; any lost-events marker counts.
    pub(super) fn on_fs_change(
        &mut self,
        paths: &[PathBuf],
        kind: FsKind,
        events: Option<&Sender<Event>>,
        watches: &mut WatchSet,
        push: &Pusher<'_>,
    ) {
        let Some(home) = &self.home else {
            return;
        };
        let home = lossy(home);
        let relevant = kind == FsKind::Rescan
            || paths
                .iter()
                .any(|p| spec::is_relevant_change(&home, &lossy(p)));
        if relevant {
            self.request_scan(events, watches, push);
        }
    }

    fn request_scan(
        &mut self,
        events: Option<&Sender<Event>>,
        watches: &mut WatchSet,
        push: &Pusher<'_>,
    ) {
        let Some(home) = self.home.clone() else {
            return;
        };
        if self.scanning {
            self.dirty = true;
            return;
        }
        match events {
            Some(tx) => {
                self.scanning = true;
                let tx = tx.clone();
                let spawned = std::thread::Builder::new()
                    .name("agent-context-scan".into())
                    .spawn(move || {
                        let _ = tx.send(Event::AgentContextScanned(Box::new(scan(&home))));
                    });
                if let Err(e) = spawned {
                    log::error!("could not start a worker thread: {e}");
                    self.scanning = false;
                }
            }
            // A core with no event channel (unit tests): scan inline.
            None => self.scanned(scan(&home), None, watches, push),
        }
    }

    /// A scan finished: follow its directories, and publish it when it differs.
    pub(super) fn scanned(
        &mut self,
        scan: Scanned,
        events: Option<&Sender<Event>>,
        watches: &mut WatchSet,
        push: &Pusher<'_>,
    ) {
        self.scanning = false;
        let newly_watched = self.update_watches(&scan.watch_dirs, watches);
        let mut snapshot = spec::assemble(&scan.home_dir, scan.skills, scan.rules, 0);
        let changed = self
            .current
            .as_ref()
            .is_none_or(|current| !spec::same_context(current, &snapshot));
        if changed {
            self.revision += 1;
            snapshot.revision = self.revision;
            log::info!(
                "Agent context revision {}: {} skill(s), {} rule file(s)",
                snapshot.revision,
                snapshot.skills.len().min(MAX_SKILLS),
                snapshot.global_rules.len()
            );
            self.current = Some(snapshot.clone());
            push.broadcast(server_message::Message::RemoteAgentContextSnapshot(
                snapshot,
            ));
        }
        // A directory that was not watched when the scan read it may have changed in between:
        // one more scan, now that it is watched, closes that gap (and finds nothing new, so
        // the chain ends).
        if std::mem::take(&mut self.dirty) || newly_watched {
            self.request_scan(events, watches, push);
        }
    }

    /// Follows `wanted`; returns whether any directory was newly watched.
    fn update_watches(&mut self, wanted: &BTreeSet<PathBuf>, watches: &mut WatchSet) -> bool {
        let mut newly = false;
        for gone in self.held.difference(wanted).cloned().collect::<Vec<_>>() {
            watches.release(&gone);
            self.held.remove(&gone);
        }
        let mut all = true;
        for dir in wanted {
            if self.held.contains(dir) {
                continue;
            }
            match watches.acquire(dir) {
                Ok(()) => {
                    self.held.insert(dir.clone());
                    newly = true;
                }
                Err(e) => {
                    if self.watched || self.current.is_none() {
                        log::warn!("Agent context is not watched: {e}");
                    }
                    all = false;
                }
            }
        }
        self.watched = all;
        newly
    }
}

#[cfg(test)]
#[path = "agent_context_tests.rs"]
mod tests;
