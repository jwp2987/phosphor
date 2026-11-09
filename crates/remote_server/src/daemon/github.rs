//! GitHub pull-request and repository info for the chips: `UpdateGitHubPrInfo`,
//! `UpdateGitHubRepoInfo` and the `GitHubPrInfoPush` / `GitHubRepositoryInfoPush` pushes (Wave H
//! parity, family e, part two). GitHub features work through the user's own `gh`.
//!
//! * `gh` is probed once (`gh --version`); when it is missing, or not signed in, or the repo has
//!   no GitHub remote, the answer is the ABSENT shape (a push with no PR / no repository), never
//!   an error. A probe that found no `gh` is repeated on an explicit request, so installing it
//!   later works without restarting the helper.
//! * PR info is looked up on an explicit request and when the branch changed since the last
//!   lookup; never on a timer and never on plain edits. The watcher's `Change::All` only asks
//!   "did the branch move?" (two cheap git calls) before it spends a `gh` network round trip.
//! * Repository info is looked up on an explicit request only.
//! * `gh` runs with `GH_PROMPT_DISABLED=1` and no stdin (the runner's contract), so it can never
//!   hang waiting for a prompt.
//!
//! The pushes echo the `repo_path` string each connection sent, as the other families do.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::Sender;

use crate::proto::{UpdateGitHubPrInfo, UpdateGitHubRepoInfo, server_message};
use crate::shellcmd::git::status::{pr_info_push, repository_info_push};
use crate::shellcmd::git::{
    GitRunner, PrInfo, RepositoryInfo, branch_for_pr_lookup, get_pr_for_branch,
    get_repository_info, is_gh_missing_error,
};

use super::git::canonical_repo_path;
use super::git_runner::HelperGitRunner;
use super::jobs::spawn_worker;
use super::repo_watch::{Change, RepoWatches};
use super::server::{ConnId, Event, Pusher};
use super::watch::WatchSet;

/// The capability this module advertises.
#[allow(dead_code)]
pub(super) const CAPABILITY: &str = crate::capability::GITHUB;

#[derive(Default)]
struct Repo {
    subs: HashMap<ConnId, String>,
    /// The client asked for PR info at least once: branch changes keep it fresh.
    wants_pr: bool,
    pr_in_flight: bool,
    pr_dirty: bool,
    pr_force: bool,
    /// The branch the last PR lookup was for.
    pr_branch: Option<Option<String>>,
    last_pr: Option<Option<PrInfo>>,
    repo_in_flight: bool,
    last_repo: Option<Option<RepositoryInfo>>,
}

/// What a worker found.
pub(super) enum Done {
    Pr {
        repo: String,
        forced: bool,
        /// `None`: the branch had not moved and nothing was asked for, so no lookup ran.
        found: Option<(Option<String>, Option<PrInfo>)>,
        /// What this run learned about `gh`; `None` when it did not look.
        gh_available: Option<bool>,
    },
    Repo {
        repo: String,
        info: Option<RepositoryInfo>,
        gh_available: bool,
    },
}

/// All GitHub subscriptions. Owned by the server core.
#[derive(Default)]
pub(super) struct Githubs {
    repos: HashMap<String, Repo>,
    /// `Some(false)` after a probe found no usable `gh`.
    gh_available: Option<bool>,
}

/// Probes `gh` once. A missing binary is an error from the runner; a `gh` that runs but fails
/// `--version` counts as unusable too.
fn probe_gh(runner: &dyn GitRunner, cwd: &std::path::Path) -> bool {
    match futures_lite::future::block_on(runner.run_gh(&["--version"], cwd)) {
        Ok(out) => out.success(),
        Err(e) => {
            log::debug!("gh is not available on this host: {e}");
            false
        }
    }
}

impl Githubs {
    fn subscribe(
        &mut self,
        conn: ConnId,
        raw: String,
        events: Option<&Sender<Event>>,
        repo_watches: &mut RepoWatches,
        watches: &mut WatchSet,
    ) -> Option<String> {
        let canonical = match canonical_repo_path(&raw) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("Invalid repo_path for a GitHub info request: {e}");
                return None;
            }
        };
        let repo = canonical.to_string_lossy().into_owned();
        if !self.repos.contains_key(&repo) {
            repo_watches.acquire(&repo, events, watches);
        }
        self.repos
            .entry(repo.clone())
            .or_default()
            .subs
            .insert(conn, raw);
        Some(repo)
    }

    /// `UpdateGitHubPrInfo` (a notification).
    pub(super) fn update_pr(
        &mut self,
        conn: ConnId,
        msg: UpdateGitHubPrInfo,
        events: Option<&Sender<Event>>,
        repo_watches: &mut RepoWatches,
        watches: &mut WatchSet,
    ) {
        let Some(repo) = self.subscribe(conn, msg.repo_path, events, repo_watches, watches) else {
            return;
        };
        let state = self.repos.get_mut(&repo).expect("just subscribed");
        state.wants_pr = true;
        state.pr_dirty = true;
        state.pr_force = true;
        self.start_pr(&repo, events);
    }

    /// `UpdateGitHubRepoInfo` (a notification).
    pub(super) fn update_repo(
        &mut self,
        conn: ConnId,
        msg: UpdateGitHubRepoInfo,
        events: Option<&Sender<Event>>,
        repo_watches: &mut RepoWatches,
        watches: &mut WatchSet,
    ) {
        let Some(repo) = self.subscribe(conn, msg.repo_path, events, repo_watches, watches) else {
            return;
        };
        let Some(tx) = events.cloned() else { return };
        let state = self.repos.get_mut(&repo).expect("just subscribed");
        if state.repo_in_flight {
            return;
        }
        state.repo_in_flight = true;
        let probe = self.gh_available;
        let repo_owned = repo.clone();
        let started = spawn_worker("github-repo", move || {
            let runner = HelperGitRunner::default();
            let path = PathBuf::from(&repo_owned);
            // Explicit request: a negative probe is repeated (gh may have been installed since).
            let available = probe == Some(true) || probe_gh(&runner, &path);
            let info = if available {
                lookup_repo(&runner, &path)
            } else {
                None
            };
            let gh_available = available;
            let _ = tx.send(Event::Github(Done::Repo {
                repo: repo_owned,
                info,
                gh_available,
            }));
        });
        if !started && let Some(state) = self.repos.get_mut(&repo) {
            state.repo_in_flight = false;
        }
    }

    fn start_pr(&mut self, repo: &str, events: Option<&Sender<Event>>) {
        let Some(tx) = events.cloned() else { return };
        let Some(state) = self.repos.get_mut(repo) else {
            return;
        };
        if state.pr_in_flight || !state.pr_dirty {
            return;
        }
        state.pr_dirty = false;
        let force = std::mem::take(&mut state.pr_force);
        state.pr_in_flight = true;
        let known_branch = state.pr_branch.clone();
        let probe = self.gh_available;
        let repo_owned = repo.to_owned();
        let started = spawn_worker("github-pr", move || {
            let runner = HelperGitRunner::default();
            let path = PathBuf::from(&repo_owned);
            let branch = futures_lite::future::block_on(branch_for_pr_lookup(&runner, &path));
            let unchanged = !force && known_branch.as_ref() == Some(&branch);
            let (found, gh_available) = if unchanged {
                (None, None)
            } else {
                // A negative probe is only trusted for non-explicit lookups.
                let available = match probe {
                    Some(true) => true,
                    Some(false) if !force => false,
                    _ => probe_gh(&runner, &path),
                };
                if available {
                    (Some((branch, lookup_pr(&runner, &path))), Some(true))
                } else {
                    (Some((branch, None)), Some(false))
                }
            };
            let _ = tx.send(Event::Github(Done::Pr {
                repo: repo_owned,
                forced: force,
                found,
                gh_available,
            }));
        });
        if !started && let Some(state) = self.repos.get_mut(repo) {
            state.pr_in_flight = false;
        }
    }

    /// The watcher saw `change` in `repo`: a moved HEAD can mean a new branch.
    pub(super) fn on_change(
        &mut self,
        repo: &str,
        change: &Change,
        events: Option<&Sender<Event>>,
    ) {
        if !matches!(change, Change::All) {
            return;
        }
        let Some(state) = self.repos.get_mut(repo) else {
            return;
        };
        if state.wants_pr {
            state.pr_dirty = true;
            self.start_pr(repo, events);
        }
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
            if state.subs.is_empty() && !state.pr_in_flight && !state.repo_in_flight {
                empty.push(repo.clone());
            }
        }
        for repo in empty {
            self.repos.remove(&repo);
            repo_watches.release(&repo, watches);
        }
    }

    /// A worker finished: push what is news, then run what queued.
    pub(super) fn done(
        &mut self,
        done: Done,
        events: Option<&Sender<Event>>,
        repo_watches: &mut RepoWatches,
        watches: &mut WatchSet,
        push: &Pusher<'_>,
    ) {
        match done {
            Done::Pr {
                repo,
                forced,
                found,
                gh_available,
            } => {
                if gh_available.is_some() {
                    self.gh_available = gh_available;
                }
                let Some(state) = self.repos.get_mut(&repo) else {
                    return;
                };
                state.pr_in_flight = false;
                if let Some((branch, pr)) = found {
                    state.pr_branch = Some(branch);
                    if forced || state.last_pr.as_ref() != Some(&pr) {
                        state.last_pr = Some(pr.clone());
                        for (conn, wire) in &state.subs {
                            push.send(
                                *conn,
                                server_message::Message::GithubPrInfoPush(pr_info_push(
                                    wire,
                                    pr.as_ref(),
                                )),
                            );
                        }
                    }
                }
                self.start_pr(&repo, events);
                self.drop_if_idle(&repo, repo_watches, watches);
            }
            Done::Repo {
                repo,
                info,
                gh_available,
            } => {
                self.gh_available = Some(gh_available);
                let Some(state) = self.repos.get_mut(&repo) else {
                    return;
                };
                state.repo_in_flight = false;
                state.last_repo = Some(info.clone());
                for (conn, wire) in &state.subs {
                    push.send(
                        *conn,
                        server_message::Message::GithubRepositoryInfoPush(repository_info_push(
                            wire,
                            info.as_ref(),
                        )),
                    );
                }
                self.drop_if_idle(&repo, repo_watches, watches);
            }
        }
    }

    fn drop_if_idle(&mut self, repo: &str, repo_watches: &mut RepoWatches, watches: &mut WatchSet) {
        let idle = self.repos.get(repo).is_some_and(|s| {
            s.subs.is_empty() && !s.pr_in_flight && !s.repo_in_flight && !s.pr_dirty
        });
        if idle {
            self.repos.remove(repo);
            repo_watches.release(repo, watches);
        }
    }
}

/// `gh pr view`, with every failure (not signed in, no network, no PR) read as "no PR".
fn lookup_pr(runner: &dyn GitRunner, repo: &std::path::Path) -> Option<PrInfo> {
    match futures_lite::future::block_on(get_pr_for_branch(runner, repo)) {
        Ok(pr) => pr,
        Err(e) => {
            log::debug!("no PR info: {e}");
            None
        }
    }
}

fn lookup_repo(runner: &dyn GitRunner, repo: &std::path::Path) -> Option<RepositoryInfo> {
    match futures_lite::future::block_on(get_repository_info(runner, repo)) {
        Ok(info) => info,
        Err(e) => {
            if is_gh_missing_error(&e.to_string()) {
                log::debug!("gh disappeared: {e}");
            } else {
                log::debug!("no repository info: {e}");
            }
            None
        }
    }
}

#[cfg(test)]
#[path = "github_tests.rs"]
mod tests;
