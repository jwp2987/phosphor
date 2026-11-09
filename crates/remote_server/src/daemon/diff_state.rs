//! Diff-state subscriptions: `GetDiffState`, the live pushes after it, and `UnsubscribeDiffState`
//! (Wave H parity, family d).
//!
//! The contract is the old full daemon's:
//!
//! * `GetDiffState{repo_path, mode}` is answered with the snapshot for the `(repo, mode)` key and
//!   subscribes the connection to later changes. The snapshot and every push echo `repo_path`
//!   verbatim, because the client keys its subscription on the exact string it sent; the daemon
//!   keys on the canonical path, so two spellings of one repo share one computation.
//! * Requests for a key that is already being computed join that computation (#324).
//! * A bad path is a `DiffStateError` response ("Invalid repo_path: ..."), not an `ErrorResponse`.
//! * `UnsubscribeDiffState` is a notification and has no reply.
//!
//! What is new is how changes are noticed. A [`RepoWatches`](super::repo_watch) event arrives as
//! [`Change`]: a few changed files are recomputed alone and pushed as `DiffStateFileDelta`s (the
//! last one carries refreshed metadata, so the counts do not go stale); HEAD, the index, a ref, a
//! new directory or too many files recompute the whole snapshot and push a `DiffStateSnapshot`.
//! One recompute runs per repository at a time; events arriving meanwhile set dirty state that
//! the next one picks up. No timer.
//!
//! All computation is [`crate::shellcmd::git::diff`], the same code the shell backend runs.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use crate::proto::{self, GetDiffState, UnsubscribeDiffState, server_message};
use crate::shellcmd::git::diff::{self, Mode};

use super::git::canonical_repo_path;
use super::git_runner::HelperGitRunner;
use super::jobs::{Reply, spawn_worker};
use super::repo_watch::{Change, RepoWatches};
use super::server::{ConnId, Event, Pusher};
use super::watch::WatchSet;

/// The capability this module advertises.
#[allow(dead_code)]
pub(super) const CAPABILITY: &str = crate::capability::DIFF_STATE;

/// A subscription key: the canonical repo and the mode.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Key {
    repo: String,
    mode: Mode,
}

/// A `GetDiffState` waiting for its key's first computation.
struct Pending {
    conn: ConnId,
    reply: Reply,
    wire: String,
}

#[derive(Default)]
struct KeyState {
    /// The `repo_path` string each subscribed connection sent.
    subs: HashMap<ConnId, String>,
    pending: Vec<Pending>,
    computing_initial: bool,
    /// Repo-relative files the client has been told have changes.
    known: HashSet<String>,
}

#[derive(Default)]
struct RepoState {
    dirty_all: bool,
    dirty_files: BTreeSet<String>,
    in_flight: bool,
}

/// What a worker computed.
pub(super) enum Done {
    /// The first snapshot of a key, answering the requests that joined it.
    Initial {
        repo: String,
        mode: Mode,
        snapshot: proto::DiffStateSnapshot,
        changed_dirs: Vec<PathBuf>,
    },
    /// A live recompute of the whole repository, one result per subscribed mode.
    Live {
        repo: String,
        results: Vec<LiveResult>,
        changed_dirs: Vec<PathBuf>,
    },
}

pub(super) struct LiveResult {
    mode: Mode,
    outcome: Outcome,
}

enum Outcome {
    Snapshot(proto::DiffStateSnapshot),
    Deltas(Vec<proto::DiffStateFileDelta>),
}

/// All diff-state subscriptions. Owned by the server core.
#[derive(Default)]
pub(super) struct DiffStates {
    keys: HashMap<Key, KeyState>,
    repos: HashMap<String, RepoState>,
}

fn error_response(message: String) -> server_message::Message {
    server_message::Message::GetDiffStateResponse(diff::error_response(message))
}

impl DiffStates {
    /// `GetDiffState`.
    pub(super) fn get_diff_state(
        &mut self,
        conn: ConnId,
        msg: GetDiffState,
        reply: Reply,
        events: Option<&Sender<Event>>,
        repo_watches: &mut RepoWatches,
        watches: &mut WatchSet,
    ) {
        let canonical = match canonical_repo_path(&msg.repo_path) {
            Ok(p) => p,
            Err(e) => {
                reply.send(error_response(format!("Invalid repo_path: {e}")));
                return;
            }
        };
        let repo = canonical.to_string_lossy().into_owned();
        let mode = Mode::from_proto(msg.mode.as_ref());
        log::info!(
            "Handling GetDiffState repo={} mode={mode:?} (request_id={})",
            msg.repo_path,
            reply.request_id()
        );
        let Some(tx) = events else {
            reply.send(error_response(
                "diff state needs the daemon's event loop".to_string(),
            ));
            return;
        };

        let key = Key {
            repo: repo.clone(),
            mode: mode.clone(),
        };
        // One watch reference per repository, however many modes and connections follow it.
        if !self.keys.keys().any(|k| k.repo == repo) {
            repo_watches.acquire(&repo, events, watches);
        }
        self.repos.entry(repo.clone()).or_default();
        let state = self.keys.entry(key).or_default();
        // Overwrites the string of an earlier subscription of the same connection.
        state.subs.insert(conn, msg.repo_path.clone());
        state.pending.push(Pending {
            conn,
            reply,
            wire: msg.repo_path,
        });
        if state.computing_initial {
            // Joins the computation in flight (#324).
            return;
        }
        state.computing_initial = true;
        let tx = tx.clone();
        let started = spawn_worker("diff-state", move || {
            let runner = HelperGitRunner::default();
            let path = PathBuf::from(&repo);
            let snapshot =
                futures_lite::future::block_on(diff::compute_snapshot(&runner, &path, "", &mode));
            let changed_dirs = changed_dirs(&path, &snapshot);
            let _ = tx.send(Event::DiffState(Done::Initial {
                repo,
                mode,
                snapshot,
                changed_dirs,
            }));
        });
        if !started {
            // Nobody will ever answer these: fail them now.
            self.fail_pending_all("could not start the computation");
        }
    }

    fn fail_pending_all(&mut self, why: &str) {
        for state in self.keys.values_mut() {
            state.computing_initial = false;
            for p in state.pending.drain(..) {
                p.reply
                    .send(error_response(format!("GetDiffState failed: {why}")));
            }
        }
    }

    /// `UnsubscribeDiffState` (a notification: no reply).
    pub(super) fn unsubscribe(
        &mut self,
        conn: ConnId,
        msg: UnsubscribeDiffState,
        repo_watches: &mut RepoWatches,
        watches: &mut WatchSet,
    ) {
        let Ok(canonical) = canonical_repo_path(&msg.repo_path) else {
            return;
        };
        let key = Key {
            repo: canonical.to_string_lossy().into_owned(),
            mode: Mode::from_proto(msg.mode.as_ref()),
        };
        self.drop_sub(&key, conn, repo_watches, watches);
    }

    fn drop_sub(
        &mut self,
        key: &Key,
        conn: ConnId,
        repo_watches: &mut RepoWatches,
        watches: &mut WatchSet,
    ) {
        let Some(state) = self.keys.get_mut(key) else {
            return;
        };
        state.subs.remove(&conn);
        state.pending.retain(|p| p.conn != conn);
        if state.subs.is_empty() && state.pending.is_empty() && !state.computing_initial {
            self.keys.remove(key);
            self.release_repo_if_unused(&key.repo, repo_watches, watches);
        }
    }

    fn release_repo_if_unused(
        &mut self,
        repo: &str,
        repo_watches: &mut RepoWatches,
        watches: &mut WatchSet,
    ) {
        if !self.keys.keys().any(|k| k.repo == repo) {
            self.repos.remove(repo);
            repo_watches.release(repo, watches);
            log::debug!("Released diff-state watch for {repo} (no subscribers left)");
        }
    }

    /// A connection went away: drop its subscriptions and queued responses.
    pub(super) fn conn_gone(
        &mut self,
        conn: ConnId,
        repo_watches: &mut RepoWatches,
        watches: &mut WatchSet,
    ) {
        let keys: Vec<Key> = self.keys.keys().cloned().collect();
        for key in keys {
            self.drop_sub(&key, conn, repo_watches, watches);
        }
    }

    /// An `Abort` for a request that may be a `GetDiffState` waiting on a computation. The
    /// computation itself goes on for the others that joined it (and for the subscription).
    pub(super) fn abort(&mut self, request_id: &str) -> bool {
        let mut found = false;
        for state in self.keys.values_mut() {
            state.pending.retain(|p| {
                let hit = p.reply.request_id() == request_id;
                found |= hit;
                !hit
            });
        }
        found
    }

    /// The watcher saw `change` in `repo`.
    pub(super) fn on_change(
        &mut self,
        repo: &str,
        change: &Change,
        events: Option<&Sender<Event>>,
    ) {
        let Some(state) = self.repos.get_mut(repo) else {
            return;
        };
        match change {
            Change::All => state.dirty_all = true,
            Change::Files(files) => state.dirty_files.extend(files.iter().cloned()),
        }
        self.start_live(repo, events);
    }

    /// Starts the one live recompute a repository may have, if anything is dirty.
    fn start_live(&mut self, repo: &str, events: Option<&Sender<Event>>) {
        let Some(tx) = events.cloned() else { return };
        let Some(state) = self.repos.get_mut(repo) else {
            return;
        };
        if state.in_flight || (!state.dirty_all && state.dirty_files.is_empty()) {
            return;
        }
        let modes: Vec<Mode> = self
            .keys
            .iter()
            .filter(|(k, v)| k.repo == repo && !v.subs.is_empty())
            .map(|(k, _)| k.mode.clone())
            .collect();
        if modes.is_empty() {
            state.dirty_all = false;
            state.dirty_files.clear();
            return;
        }
        let all = std::mem::take(&mut state.dirty_all);
        let files: Vec<String> = std::mem::take(&mut state.dirty_files).into_iter().collect();
        state.in_flight = true;
        let repo = repo.to_owned();
        let repo_if_it_fails = repo.clone();
        let started = spawn_worker("diff-state-live", move || {
            let runner = HelperGitRunner::default();
            let path = PathBuf::from(&repo);
            let mut results = Vec::new();
            let mut changed_dirs = Vec::new();
            for mode in modes {
                let outcome = if all {
                    let snap = futures_lite::future::block_on(diff::compute_snapshot(
                        &runner, &path, "", &mode,
                    ));
                    changed_dirs.extend(self::changed_dirs(&path, &snap));
                    Outcome::Snapshot(snap)
                } else {
                    let mut deltas = futures_lite::future::block_on(diff::compute_file_deltas(
                        &runner, &path, "", &mode, &files,
                    ));
                    // The counts move with the files: ride the refreshed metadata on the last
                    // delta (the old daemon left them stale until the next whole push).
                    if !deltas.is_empty()
                        && let Ok(meta) = futures_lite::future::block_on(diff::load_metadata(
                            &runner,
                            &path,
                            mode.includes_base_branch(),
                        ))
                        && let Some(last) = deltas.last_mut()
                    {
                        last.metadata = Some(meta);
                    }
                    for d in &deltas {
                        if d.diff.is_some()
                            && let Some(parent) = Path::new(&d.file_path).parent()
                        {
                            changed_dirs.push(path.join(parent));
                        }
                    }
                    Outcome::Deltas(deltas)
                };
                results.push(LiveResult { mode, outcome });
            }
            let _ = tx.send(Event::DiffState(Done::Live {
                repo,
                results,
                changed_dirs,
            }));
        });
        if !started && let Some(state) = self.repos.get_mut(&repo_if_it_fails) {
            state.in_flight = false;
        }
    }

    /// A worker finished.
    pub(super) fn done(
        &mut self,
        done: Done,
        events: Option<&Sender<Event>>,
        repo_watches: &mut RepoWatches,
        watches: &mut WatchSet,
        push: &Pusher<'_>,
    ) {
        match done {
            Done::Initial {
                repo,
                mode,
                snapshot,
                changed_dirs,
            } => {
                repo_watches.add_dirs(&repo, changed_dirs, watches);
                let key = Key { repo, mode };
                let Some(state) = self.keys.get_mut(&key) else {
                    return;
                };
                state.computing_initial = false;
                state.known = known_files(&snapshot);
                for pending in std::mem::take(&mut state.pending) {
                    let mut snap = snapshot.clone();
                    snap.repo_path = pending.wire;
                    pending
                        .reply
                        .send(server_message::Message::GetDiffStateResponse(
                            diff::snapshot_response(snap),
                        ));
                }
                // Everyone who asked has gone meanwhile: nothing follows this key any more.
                if state.subs.is_empty() {
                    let repo = key.repo.clone();
                    self.keys.remove(&key);
                    self.release_repo_if_unused(&repo, repo_watches, watches);
                }
            }
            Done::Live {
                repo,
                results,
                changed_dirs,
            } => {
                repo_watches.add_dirs(&repo, changed_dirs, watches);
                if let Some(state) = self.repos.get_mut(&repo) {
                    state.in_flight = false;
                }
                for result in results {
                    let key = Key {
                        repo: repo.clone(),
                        mode: result.mode,
                    };
                    let Some(state) = self.keys.get_mut(&key) else {
                        continue;
                    };
                    match result.outcome {
                        Outcome::Snapshot(snapshot) => {
                            state.known = known_files(&snapshot);
                            for (conn, wire) in &state.subs {
                                let mut snap = snapshot.clone();
                                snap.repo_path = wire.clone();
                                push.send(*conn, server_message::Message::DiffStateSnapshot(snap));
                            }
                        }
                        Outcome::Deltas(deltas) => {
                            for delta in deltas {
                                // A file that was not part of the diff and still is not (an
                                // ignored build output) is not news.
                                let was_known = state.known.contains(&delta.file_path);
                                if delta.diff.is_none() && !was_known {
                                    continue;
                                }
                                if delta.diff.is_some() {
                                    state.known.insert(delta.file_path.clone());
                                } else {
                                    state.known.remove(&delta.file_path);
                                }
                                for (conn, wire) in &state.subs {
                                    let mut d = delta.clone();
                                    d.repo_path = wire.clone();
                                    push.send(
                                        *conn,
                                        server_message::Message::DiffStateFileDelta(d),
                                    );
                                }
                            }
                        }
                    }
                }
                // Whatever changed while this ran.
                self.start_live(&repo, events);
            }
        }
    }
}

fn known_files(snapshot: &proto::DiffStateSnapshot) -> HashSet<String> {
    snapshot
        .diffs
        .iter()
        .flat_map(|d| d.files.iter().map(|f| f.file_path.clone()))
        .collect()
}

/// The directories of the files a snapshot lists, to watch them even when git does not track
/// them yet.
fn changed_dirs(repo: &Path, snapshot: &proto::DiffStateSnapshot) -> Vec<PathBuf> {
    let mut dirs: BTreeSet<PathBuf> = BTreeSet::new();
    for file in snapshot.diffs.iter().flat_map(|d| &d.files) {
        if let Some(parent) = Path::new(file.file_path.trim_end_matches('/')).parent() {
            dirs.insert(repo.join(parent));
        }
    }
    dirs.into_iter().collect()
}

#[cfg(test)]
#[path = "diff_state_tests.rs"]
mod tests;
