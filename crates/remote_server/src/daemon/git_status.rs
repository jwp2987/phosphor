//! The aggregate git status behind the prompt's git chip: the `UpdateGitStatus` notification and
//! the `GitStatusPush` pushes it asks for (Wave H parity, family e, part one).
//!
//! `UpdateGitStatus{repo_path}` subscribes the connection to the repository's status and forces
//! one push of the current value, even if unchanged. After that a push goes out whenever the
//! status differs from the last one pushed (#1268): the watcher events of
//! [`RepoWatches`](super::repo_watch) mark the repo dirty, one compute runs at a time and a
//! change arriving meanwhile queues exactly one more. The pushes echo the `repo_path` string each
//! connection sent. The main branch is detected once and kept until HEAD or a ref moves.
//!
//! The values are [`crate::shellcmd::git::status`], the code the shell backend runs too.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::Sender;

use crate::proto::{self, UpdateGitStatus, server_message};
use crate::shellcmd::git::status::{compute_git_status_metadata, git_status_push};

use super::git::canonical_repo_path;
use super::git_runner::HelperGitRunner;
use super::jobs::spawn_worker;
use super::repo_watch::{Change, RepoWatches};
use super::server::{ConnId, Event, Pusher};
use super::watch::WatchSet;

/// The capability this module advertises.
#[allow(dead_code)]
pub(super) const CAPABILITY: &str = crate::capability::GIT_STATUS;

#[derive(Default)]
struct Repo {
    /// The `repo_path` string each subscribed connection sent.
    subs: HashMap<ConnId, String>,
    in_flight: bool,
    /// A change or request arrived while a compute ran.
    dirty: bool,
    /// ... and at least one of them wants a push even if the value is unchanged.
    force: bool,
    main_branch: Option<String>,
    last_pushed: Option<proto::GitStatusMetadata>,
}

/// A finished compute.
pub(super) struct Done {
    repo: String,
    forced: bool,
    result: Result<proto::GitStatusMetadata, String>,
}

/// All status subscriptions. Owned by the server core.
#[derive(Default)]
pub(super) struct GitStatuses {
    repos: HashMap<String, Repo>,
}

impl GitStatuses {
    /// `UpdateGitStatus` (a notification: nothing is replied).
    pub(super) fn update(
        &mut self,
        conn: ConnId,
        msg: UpdateGitStatus,
        events: Option<&Sender<Event>>,
        repo_watches: &mut RepoWatches,
        watches: &mut WatchSet,
    ) {
        let canonical = match canonical_repo_path(&msg.repo_path) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("Invalid repo_path for UpdateGitStatus: {e}");
                return;
            }
        };
        let repo = canonical.to_string_lossy().into_owned();
        if !self.repos.contains_key(&repo) {
            repo_watches.acquire(&repo, events, watches);
        }
        let state = self.repos.entry(repo.clone()).or_default();
        state.subs.insert(conn, msg.repo_path);
        state.dirty = true;
        state.force = true;
        self.start(&repo, events);
    }

    fn start(&mut self, repo: &str, events: Option<&Sender<Event>>) {
        let Some(tx) = events.cloned() else { return };
        let Some(state) = self.repos.get_mut(repo) else {
            return;
        };
        if state.in_flight || !state.dirty {
            return;
        }
        state.dirty = false;
        let forced = std::mem::take(&mut state.force);
        state.in_flight = true;
        let cached_main = state.main_branch.clone();
        let repo_owned = repo.to_owned();
        let started = spawn_worker("git-status", move || {
            let runner = HelperGitRunner::default();
            let path = PathBuf::from(&repo_owned);
            let result = futures_lite::future::block_on(compute_git_status_metadata(
                &runner,
                &path,
                cached_main,
            ))
            .map_err(|e| e.to_string());
            let _ = tx.send(Event::GitStatus(Done {
                repo: repo_owned,
                forced,
                result,
            }));
        });
        if !started && let Some(state) = self.repos.get_mut(repo) {
            state.in_flight = false;
        }
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
        if matches!(change, Change::All) {
            // HEAD or a ref moved: the main branch may have too.
            state.main_branch = None;
        }
        state.dirty = true;
        self.start(repo, events);
    }

    /// A connection went away.
    pub(super) fn conn_gone(
        &mut self,
        conn: ConnId,
        repo_watches: &mut RepoWatches,
        watches: &mut WatchSet,
    ) {
        let mut empty = Vec::new();
        for (repo, state) in &mut self.repos {
            state.subs.remove(&conn);
            if state.subs.is_empty() && !state.in_flight {
                empty.push(repo.clone());
            }
        }
        for repo in empty {
            self.repos.remove(&repo);
            repo_watches.release(&repo, watches);
        }
    }

    /// A compute finished: push if the value is news (or was asked for), then run what queued.
    pub(super) fn done(
        &mut self,
        done: Done,
        events: Option<&Sender<Event>>,
        repo_watches: &mut RepoWatches,
        watches: &mut WatchSet,
        push: &Pusher<'_>,
    ) {
        let Done {
            repo,
            forced,
            result,
        } = done;
        let Some(state) = self.repos.get_mut(&repo) else {
            return;
        };
        state.in_flight = false;
        match result {
            Ok(metadata) => {
                if state.main_branch.is_none() {
                    state.main_branch = Some(metadata.main_branch_name.clone());
                }
                if forced || state.last_pushed.as_ref() != Some(&metadata) {
                    state.last_pushed = Some(metadata.clone());
                    for (conn, wire) in &state.subs {
                        push.send(
                            *conn,
                            server_message::Message::GitStatusPush(git_status_push(
                                wire,
                                metadata.clone(),
                            )),
                        );
                    }
                }
            }
            // Not a repository, or git failed: say nothing; the client keeps what it had, and
            // the next change or request tries again.
            Err(e) => log::debug!("git status for {repo} failed: {e}"),
        }
        if state.subs.is_empty() && !state.dirty {
            self.repos.remove(&repo);
            repo_watches.release(&repo, watches);
            return;
        }
        self.start(&repo, events);
    }
}

#[cfg(test)]
#[path = "git_status_tests.rs"]
mod tests;
