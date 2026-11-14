//! Family e on the shell arm: the git chip's status and the GitHub PR / repository info, over the
//! out-of-band channel.
//!
//! Unlike the helper, which is told once (`UpdateGitStatus`, ...) and pushes from then on, these
//! are plain calls that return the current answer; the app calls them again on refresh events.
//! The values come from the same code the helper runs ([`crate::shellcmd::git::status`],
//! [`crate::shellcmd::git::gh`]).
//!
//! GitHub works through the user's own `gh` on the host. A `gh` that is missing, not signed in,
//! or a repository with no GitHub remote is the ABSENT answer (a push with no PR / no repository),
//! never an error. PR info is only looked up when asked for explicitly or when the branch moved
//! since the caller's last lookup; the branch check is two cheap git calls, `gh` is the network
//! round trip.

use std::path::Path;

use super::super::{BackendError, Family, ShellBackend};
use super::git_batch::{BatchedChip, ChipCommandOutput};
use crate::proto::{GitHubPrInfoPush, GitHubRepositoryInfoPush, GitStatusPush};
use crate::shellcmd::git::status::{
    compute_git_status_metadata, git_status_push, pr_info_push, repository_info_push,
};
use crate::shellcmd::git::{branch_for_pr_lookup, get_pr_for_branch, get_repository_info};

/// What a PR lookup found.
#[derive(Clone, Debug, PartialEq)]
pub enum PrLookup {
    /// The branch had not moved and nothing was asked for: the caller's value stands.
    Unchanged,
    Fetched {
        /// The branch the lookup was for (`None`: detached or not a repository). Hand it back as
        /// `known_branch` next time.
        branch: Option<String>,
        push: GitHubPrInfoPush,
    },
}

impl ShellBackend {
    /// What a prompt chip's own command prints in `dir` (the session's working directory), read
    /// from the prompt's batched exec instead of run as one more exec channel (#2216).
    ///
    /// * `Ok(Some(..))`: the command's output and status, from the batch.
    /// * `Ok(None)`: the batch does not answer here (the kill switch, a PowerShell host, or the
    ///   Kubernetes chip's first ask on this host): the caller runs the chip's command itself.
    /// * `Err`: this session batches and the prompt's batch has no answer (its exec failed). No
    ///   second exec is run for it; the chip has no value for this prompt.
    ///
    /// The batch is the one of the repository `dir` is in when a navigation has resolved it
    /// (the chips, the tree refresh and this then share ONE exec in a subdirectory too), else
    /// the one of `dir` itself.
    pub async fn batched_chip_shell(
        &self,
        dir: &str,
        chip: BatchedChip,
    ) -> Result<Option<ChipCommandOutput>, BackendError> {
        if self.batch_git_off || self.shell_kind().is_powershell() {
            return Ok(None);
        }
        let inner = self.git_runner("prompt chip")?;
        if chip == BatchedChip::KubeContext && !self.git_batch.want_kube() {
            return Ok(None);
        }
        let key = self
            .tree_cache
            .navigated_root(dir)
            .unwrap_or_else(|| dir.to_owned());
        let answer = self
            .git_batch
            .get(self, &inner, &key, chip.producer())
            .await
            .and_then(|batch| batch.chip(chip));
        answer.map(Some).ok_or_else(|| {
            BackendError::Unavailable(format!(
                "{}: the prompt's batched read has no answer",
                chip.producer()
            ))
        })
    }

    /// The chip's status of `repo_path`. `Ok(None)`: nothing to report (not a repository, or git
    /// failed), exactly when the helper stays silent. `cached_main_branch` skips the main-branch
    /// detection (several git calls) when the caller still trusts an earlier answer.
    pub async fn git_status_shell(
        &self,
        repo_path: &str,
        cached_main_branch: Option<String>,
    ) -> Result<Option<GitStatusPush>, BackendError> {
        log::debug!("shell arm serving {:?}", Family::GitStatus);
        let runner = self.batched_git_runner("git_status", repo_path).await?;
        match compute_git_status_metadata(&runner, Path::new(repo_path), cached_main_branch).await {
            Ok(metadata) => Ok(Some(git_status_push(repo_path, metadata))),
            Err(e) => {
                log::debug!("git status over the shell failed: {e}");
                Ok(None)
            }
        }
    }

    /// PR info for the current branch of `repo_path`. With `force` it always asks `gh`; without,
    /// only when the branch differs from `known_branch` (`None`: no lookup yet).
    pub async fn pr_info_shell(
        &self,
        repo_path: &str,
        known_branch: Option<Option<String>>,
        force: bool,
    ) -> Result<PrLookup, BackendError> {
        log::debug!("shell arm serving {:?}", Family::Github);
        let runner = self.batched_git_runner("pr_info", repo_path).await?;
        let repo = Path::new(repo_path);
        let branch = branch_for_pr_lookup(&runner, repo).await;
        if !force && known_branch.as_ref() == Some(&branch) {
            return Ok(PrLookup::Unchanged);
        }
        let pr = match get_pr_for_branch(&runner, repo).await {
            Ok(pr) => pr,
            Err(e) => {
                // Not signed in, no network, no gh: the absent shape.
                log::debug!("no PR info over the shell: {e}");
                None
            }
        };
        Ok(PrLookup::Fetched {
            branch,
            push: pr_info_push(repo_path, pr.as_ref()),
        })
    }

    /// Repository name / owner / host of `repo_path` from `gh repo view`; absent when there is
    /// none to give.
    pub async fn repository_info_shell(
        &self,
        repo_path: &str,
    ) -> Result<GitHubRepositoryInfoPush, BackendError> {
        log::debug!("shell arm serving {:?}", Family::Github);
        let runner = self
            .batched_git_runner("repository_info", repo_path)
            .await?;
        let info = match get_repository_info(&runner, Path::new(repo_path)).await {
            Ok(info) => info,
            Err(e) => {
                log::debug!("no repository info over the shell: {e}");
                None
            }
        };
        Ok(repository_info_push(repo_path, info.as_ref()))
    }
}
